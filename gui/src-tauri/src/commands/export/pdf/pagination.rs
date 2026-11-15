//! Add consistent page counters without depending on CSS margin-box support.
//! Preserve the engine's text, fonts, links and page streams. Each footer gets
//! its own graphics state and a page-local resource dictionary, including when
//! the renderer put the original resources on a shared ancestor.
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

pub(super) fn number_pages(bytes: &[u8]) -> Result<Vec<u8>, String> {
    finish(bytes).map_err(|e| format!("Could not finish PDF pagination: {e}"))
}

fn finish(bytes: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut pdf = Document::load_mem(bytes)?;
    let pages = pdf.get_pages();
    if pages.is_empty() || pages.len() > 10_000 {
        return Err("Invalid report page count".into());
    }
    let font = pdf.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let start = pdf.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    for (number, &page) in &pages {
        let mut resources = resources(&pdf, page)?;
        let mut fonts = match resources.get(b"Font") {
            Ok(object) => pdf.dereference(object)?.1.as_dict()?.clone(),
            Err(_) => Dictionary::new(),
        };
        // Engines choose their own resource names; do not overwrite one.
        let mut name = "PipelinePageNumber".to_owned();
        while fonts.has(name.as_bytes()) {
            name.push('_');
        }
        fonts.set(name.as_bytes(), font);
        resources.set("Font", fonts);
        let label = format!("{number} / {}", pages.len());
        let width: f64 = label
            .chars()
            .map(|c| if c.is_ascii_digit() { 0.556 } else { 0.278 })
            .sum::<f64>()
            * 8.;
        let x = (super::native::WIDTH - width) / 2.;
        let footer = format!(
            "Q\nq\nBT /{name} 8 Tf 0.48 0.51 0.54 rg 1 0 0 1 {x:.3} 18 Tm ({label}) Tj ET\nQ\n"
        );
        let end = pdf.add_object(Stream::new(Dictionary::new(), footer.into_bytes()));
        let contents: Vec<Object> = std::iter::once(start)
            .chain(pdf.get_page_contents(page))
            .chain(std::iter::once(end))
            .map(Object::Reference)
            .collect();
        let page = pdf.get_dictionary_mut(page)?;
        page.set("Resources", resources);
        page.set("Contents", contents);
    }
    let mut output = Vec::new();
    pdf.save_to(&mut output)?;
    if output.len() as u64 > super::MAX_PDF {
        return Err("The PDF exceeds the 64 MB export limit. Save Markdown instead.".into());
    }
    Ok(output)
}

fn resources(pdf: &Document, mut page: ObjectId) -> lopdf::Result<Dictionary> {
    // Stop at the nearest inherited Resources entry, as required by PDF.
    for _ in 0..64 {
        let node = pdf.get_dictionary(page)?;
        if let Ok(resources) = node.get(b"Resources") {
            return Ok(pdf.dereference(resources)?.1.as_dict()?.clone());
        }
        match node.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent) => page = parent,
            Err(_) => return Ok(Dictionary::new()),
        }
    }
    Err(lopdf::Error::ReferenceCycle(page))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_preserve_inherited_resources_and_existing_graphics() {
        let mut pdf = Document::with_version("1.5");
        let pages = pdf.new_object_id();
        let existing_font = pdf.add_object(
            dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier"},
        );
        let contents = pdf.add_object(Stream::new(
            Dictionary::new(),
            b"2 0 0 2 0 0 cm\nBT /PipelinePageNumber 10 Tf 40 300 Td (Original text) Tj ET"
                .to_vec(),
        ));
        let ids: Vec<_> = (0..2)
            .map(|_| {
                pdf.add_object(
                    dictionary! {"Type" => "Page", "Parent" => pages, "Contents" => contents},
                )
            })
            .collect();
        pdf.objects.insert(pages, dictionary! {
            "Type" => "Pages", "Kids" => ids.iter().copied().map(Object::Reference).collect::<Vec<_>>(), "Count" => 2,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {"Font" => dictionary! {"PipelinePageNumber" => existing_font}}
        }.into());
        let catalog = pdf.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
        pdf.trailer.set("Root", catalog);
        let mut bytes = vec![];
        pdf.save_to(&mut bytes).unwrap();
        let result = number_pages(&bytes).unwrap();
        let numbered = Document::load_mem(&result).unwrap();
        assert_eq!(numbered.get_pages().len(), 2);
        for (number, id) in numbered.get_pages() {
            let text = numbered.extract_text(&[number]).unwrap();
            assert!(text.contains("Original text"));
            assert!(text.contains(&format!("{number} / 2")));
            let streams = numbered.get_page_contents(id);
            assert_eq!(streams.len(), 3);
            let resource = resources(&numbered, id).unwrap();
            assert_eq!(resource.get(b"Font").unwrap().as_dict().unwrap().len(), 2);
        }
    }

    #[test]
    fn corrupt_output_is_rejected() {
        assert!(number_pages(b"%PDF- broken").is_err());
    }
}
