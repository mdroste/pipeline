//! XML extraction preserves run separators and complete math subtrees.
use quick_xml::{events::Event, Reader, Writer};
use std::collections::BTreeMap;

pub(super) fn text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    let mut in_text = false;
    let mut output = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"w:t" | b"m:t" => in_text = true,
                b"w:tab" => output.push('\t'),
                b"w:br" | b"w:cr" => output.push('\n'),
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"w:tab" => output.push('\t'),
                b"w:br" | b"w:cr" => output.push('\n'),
                _ => {}
            },
            Ok(Event::End(e)) if matches!(e.name().as_ref(), b"w:t" | b"m:t") => in_text = false,
            Ok(Event::Text(e)) if in_text => {
                if let Ok(value) = e.decode() {
                    output.push_str(&value);
                }
            }
            Ok(Event::CData(e)) if in_text => {
                if let Ok(value) = e.decode() {
                    output.push_str(&value);
                }
            }
            Ok(Event::GeneralRef(e)) if in_text => {
                if let Ok(Some(c)) = e.resolve_char_ref() {
                    output.push(c);
                } else if let Ok(name) = e.decode() {
                    let encoded = format!("&{name};");
                    if let Ok(value) = quick_xml::escape::unescape(&encoded) {
                        output.push_str(&value);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    output
}

pub(super) fn equations(
    xml: &str,
    byte_limit: usize,
    count_limit: usize,
) -> Result<(Vec<String>, usize), String> {
    let mut reader = Reader::from_str(xml);
    let mut namespaces: Vec<BTreeMap<String, String>> = vec![BTreeMap::new()];
    let mut capture: Option<(usize, Writer<Vec<u8>>)> = None;
    let mut output = Vec::new();
    let mut omitted = 0;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("Invalid DOCX XML: {e}"))?;
        match &event {
            Event::Start(e) => {
                let mut scope = namespaces.last().cloned().unwrap_or_default();
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| format!("Invalid DOCX attribute: {e}"))?;
                    if attr.key.as_ref() == b"xmlns" || attr.key.as_ref().starts_with(b"xmlns:") {
                        scope.insert(
                            String::from_utf8_lossy(attr.key.as_ref()).into_owned(),
                            String::from_utf8_lossy(&attr.value).into_owned(),
                        );
                    }
                }
                if namespaces.len() >= 512
                    || scope.len() > 128
                    || scope
                        .iter()
                        .map(|(key, value)| key.len() + value.len())
                        .sum::<usize>()
                        > 64 * 1024
                {
                    return Err("DOCX XML exceeds nesting or namespace limits".into());
                }
                namespaces.push(scope);
                if capture.is_none() && matches!(e.name().as_ref(), b"m:oMath" | b"m:oMathPara") {
                    let mut start = e.to_owned();
                    let present: Vec<_> = start
                        .attributes()
                        .filter_map(Result::ok)
                        .map(|a| a.key.as_ref().to_vec())
                        .collect();
                    for (key, value) in namespaces.last().unwrap() {
                        if !present.iter().any(|k| k == key.as_bytes()) {
                            start.push_attribute((key.as_str(), value.as_str()));
                        }
                    }
                    let mut writer = Writer::new(Vec::new());
                    writer
                        .write_event(Event::Start(start))
                        .map_err(|e| e.to_string())?;
                    capture = Some((namespaces.len(), writer));
                    continue;
                }
            }
            Event::End(_) => {
                if let Some((depth, writer)) = capture.as_mut() {
                    writer
                        .write_event(event.clone())
                        .map_err(|e| e.to_string())?;
                    if *depth == namespaces.len() {
                        let (_, writer) = capture.take().unwrap();
                        let bytes = writer.into_inner();
                        if bytes.len() <= byte_limit && output.len() < count_limit {
                            output.push(String::from_utf8(bytes).map_err(|e| e.to_string())?);
                        } else {
                            omitted += 1;
                        }
                    }
                    namespaces.pop();
                    continue;
                }
                namespaces.pop();
            }
            Event::DocType(_) => return Err("DOCX XML must not contain a DTD".into()),
            Event::Eof => break,
            _ => {}
        }
        if let Some((_, writer)) = capture.as_mut() {
            writer.write_event(event).map_err(|e| e.to_string())?;
        }
    }
    if capture.is_some() || namespaces.len() != 1 {
        return Err("DOCX XML contains an unclosed element".into());
    }
    Ok((output, omitted))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_namespace_stack_growth() {
        let xml = format!("{}{}", "<w:p>".repeat(600), "</w:p>".repeat(600));
        assert!(equations(&xml, 1000, 10).unwrap_err().contains("limits"));
    }
    #[test]
    fn preserves_text_separators_and_numeric_entities() {
        assert_eq!(text("<w:p><w:r><w:t>before</w:t><w:tab/><w:t>after</w:t><w:br/><w:t>next &#x3B1; &#945; &amp;</w:t></w:r></w:p>"), "before\tafter\nnext α α &");
        assert_eq!(
            text("<w:r><w:t>con</w:t></w:r><w:r><w:t>tiguous</w:t></w:r>"),
            "contiguous"
        );
    }
    #[test]
    fn math_is_balanced_namespaced_and_never_truncated() {
        let xml = r#"<w:document xmlns:w="word" xmlns:m="math"><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath><m:oMath><m:r><m:t>y</m:t></m:r></m:oMath></m:oMathPara></w:document>"#;
        let (math, omitted) = equations(xml, 1000, 10).unwrap();
        assert_eq!(math.len(), 1);
        assert_eq!(omitted, 0);
        assert!(math[0].ends_with("</m:oMathPara>"));
        assert!(math[0].contains("xmlns:m=\"math\""));
        assert_eq!(equations(&math[0], 1000, 10).unwrap().0.len(), 1);
        assert_eq!(equations(xml, 5, 10).unwrap(), (vec![], 1));
    }
}
