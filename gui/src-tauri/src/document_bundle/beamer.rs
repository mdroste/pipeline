//! Source frame references deliberately remain separate from PDF page numbers:
//! overlays, repeated frames, and frame breaks can produce multiple PDF pages.
use super::*;
use std::ops::Range;

fn frame_ranges(source: &str) -> Vec<Range<usize>> {
    let command = Regex::new(r"\\begin\s*\{frame\}|\\frame\b").unwrap();
    let end_frame = Regex::new(r"\\end\s*\{frame\}").unwrap();
    let mut ranges = Vec::new();
    // Definitions in the preamble are not presented frames.
    let mut consumed = source
        .find(r"\begin{document}")
        .map_or(0, |start| start + r"\begin{document}".len());
    for found in command.find_iter(source) {
        if found.start() < consumed {
            continue;
        }
        let tail = &source[found.end()..];
        let end = if found.as_str().starts_with("\\begin") {
            end_frame.find(tail).map(|end| found.end() + end.end())
        } else {
            // Skip overlay specifications and options before the command body.
            let mut offset = 0;
            let bytes = tail.as_bytes();
            while offset < bytes.len() {
                match bytes[offset] {
                    b' ' | b'\n' | b'\r' | b'\t' => offset += 1,
                    b'<' | b'[' => {
                        let close = if bytes[offset] == b'<' { b'>' } else { b']' };
                        let Some(end) = bytes[offset + 1..].iter().position(|b| *b == close) else {
                            break;
                        };
                        offset += end + 2;
                    }
                    _ => break,
                }
            }
            if bytes.get(offset) != Some(&b'{') {
                continue;
            }
            let mut depth = 0usize;
            let mut escaped = false;
            let mut end = None;
            for (i, byte) in bytes.iter().enumerate().skip(offset) {
                if escaped {
                    escaped = false;
                    continue;
                }
                if *byte == b'\\' {
                    escaped = true;
                    continue;
                }
                if *byte == b'{' {
                    depth += 1;
                }
                if *byte == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(found.end() + i + 1);
                        break;
                    }
                }
            }
            end
        };
        if let Some(end) = end {
            ranges.push(found.start()..end);
            consumed = end;
        }
    }
    ranges
}

fn visible_beamer(source: &str) -> Option<String> {
    // Mask comments without moving byte offsets, so annotations retain the
    // author's exact source (including comments and verbatim examples).
    let mut visible = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        let uncommented = crate::pipeline::extract::latex_without_comments(line);
        let prefix = uncommented.trim_end_matches('\n');
        visible.push_str(prefix);
        let newline = usize::from(line.ends_with('\n'));
        visible.push_str(&" ".repeat(line.len() - prefix.len() - newline));
        if newline != 0 {
            visible.push('\n');
        }
    }
    Regex::new(r"\\documentclass\s*(?:\[[^\]]*\]\s*)?\{beamer\}")
        .unwrap()
        .is_match(&visible)
        .then_some(visible)
}

pub(crate) fn annotate(source: String, warnings: &mut Vec<String>) -> String {
    let Some(visible) = visible_beamer(&source) else {
        return source;
    };
    warnings.push("Beamer source review: cite source frame numbers and titles. Source frames are not PDF page numbers: overlays, repeated frames, and frame breaks may change pagination. TeX macros are not expanded or compiled; verify visual layout against a compiled PDF.".into());
    let ranges = frame_ranges(&visible);
    let mut result = String::new();
    let mut last = 0;
    for (index, range) in ranges.iter().enumerate() {
        result.push_str(&source[last..range.start]);
        result.push_str(&format!("\n% Beamer source frame {}\n", index + 1));
        result.push_str(&source[range.clone()]);
        last = range.end;
    }
    result.push_str(&source[last..]);
    result
}

pub(super) fn add_frames(bundle: &mut DocumentBundle, source: &str) {
    let Some(visible) = visible_beamer(source) else {
        return;
    };
    for (index, range) in frame_ranges(&visible).into_iter().enumerate() {
        let text = source[range].to_string();
        let frame = node(
            bundle,
            "slide",
            None,
            Some(format!("Source frame {}", index + 1)),
            Some((index + 1).to_string()),
            text.clone(),
            vec![],
            vec![DocumentRepresentation {
                format: "latex".into(),
                content: text.into(),
            }],
            "latex",
        );
        bundle.nodes.push(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beamer_frames_preserve_math_and_do_not_claim_pdf_pages() {
        let source = r"\documentclass[aspectratio=169]{beamer}
% \begin{frame} ignored \end{frame}
\begin{frame}<1-2>[fragile]{Model} $y=\alpha x$ \pause More \end{frame}
\frame<2>[plain]{\frametitle{Results} {Nested} \{literal\}}
\againframe{model}";
        let mut warnings = vec![];
        let annotated = annotate(source.into(), &mut warnings);
        assert_eq!(annotated.matches("% Beamer source frame").count(), 2);
        assert!(annotated.contains(r"$y=\alpha x$"));
        assert!(annotated.contains("ignored"));
        assert!(warnings[0].contains("not PDF page numbers"));
        assert_eq!(frame_ranges(&visible_beamer(&annotated).unwrap()).len(), 2);
        let paper = r"\documentclass{article}\begin{frame}example\end{frame}";
        assert_eq!(annotate(paper.into(), &mut vec![]), paper);
    }
    #[tokio::test]
    async fn beamer_project_staging_expands_frames_and_keeps_bundle_references() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("talk.tex"),
            r"\documentclass{beamer}\input{results}",
        )
        .unwrap();
        fs::write(
            dir.path().join("results.tex"),
            r"\begin{frame}{Result}$x=1$\end{frame}",
        )
        .unwrap();
        let private = tempfile::tempdir().unwrap();
        let scoped =
            crate::pipeline::extract::stage_selected_source(dir.path(), "document", private.path())
                .unwrap();
        let app: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let result = crate::pipeline::extract::extract(
            &app,
            scoped.source_path.unwrap().to_str().unwrap(),
            &crate::pipeline_config::ExtractionConfig::default(),
        )
        .await
        .unwrap();
        assert!(result.text.contains("Beamer source frame 1"));
        assert!(result.text.contains("$x=1$"));
        let run = tempfile::tempdir().unwrap();
        let built = build(&result, run.path()).unwrap();
        let frame = built
            .bundle
            .nodes
            .iter()
            .find(|node| node.kind == "slide")
            .unwrap();
        assert_eq!(frame.number.as_deref(), Some("1"));
        assert_eq!(frame.page, None);
        assert!(frame.text.contains("Result"));
    }
}
