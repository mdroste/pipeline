use super::*;

fn paddle_node_kind(block: &crate::pipeline::extract::PaddleStructuredBlock) -> &'static str {
    let label = if block.block_label.is_empty() {
        block.role.as_str()
    } else {
        block.block_label.as_str()
    };
    match label.to_ascii_lowercase().as_str() {
        "footnote" => "footnote",
        "possible_footnote" => "possible_footnote",
        "page_header" | "header" | "header_image" => "page_header",
        "page_footer" | "footer" | "footer_image" | "number" => "page_footer",
        "doc_title" | "document_title" | "paragraph_title" | "section_title" | "abstract_title"
        | "reference_title" => "section",
        "formula" | "display_formula" | "inline_formula" | "equation" => "equation",
        // A formula number is a separate layout region, not a second equation.
        // `add_paddle_structured_nodes` attaches adjacent numbers to their
        // equation node and retains unmatched regions as ordinary text.
        "formula_number" => "text_block",
        "table" | "table_body" => "table",
        "image" | "figure" | "chart" | "seal" => "figure",
        "caption" | "figure_title" | "image_caption" | "table_title" | "table_caption"
        | "chart_title" | "vision_footnote" => "caption",
        "code" | "algorithm" => "code",
        "list" | "list_item" => "list",
        _ => {
            let markdown = block.markdown.trim();
            let lines: Vec<&str> = markdown.lines().collect();
            if Regex::new(r"^#{1,6}\s+").unwrap().is_match(markdown) {
                "section"
            } else if markdown.starts_with("```") || markdown.starts_with("~~~") {
                "code"
            } else if markdown.starts_with("$$")
                || markdown.starts_with(r"\[")
                || (markdown.starts_with('$') && markdown.ends_with('$'))
            {
                "equation"
            } else if lines.len() >= 2 && lines[0].contains('|') && is_markdown_separator(lines[1])
            {
                "table"
            } else if Regex::new(r"(?i)^(figure|fig\.?|table)\s+[A-Z]?\d")
                .unwrap()
                .is_match(markdown)
            {
                "caption"
            } else if Regex::new(r"(?m)^\s*(?:[-+*]|\d+[.)])\s+")
                .unwrap()
                .is_match(markdown)
            {
                "list"
            } else {
                "text_block"
            }
        }
    }
}

fn paddle_block_label(block: &crate::pipeline::extract::PaddleStructuredBlock) -> &str {
    if block.block_label.is_empty() {
        block.role.as_str()
    } else {
        block.block_label.as_str()
    }
}

fn is_paddle_formula_number(block: &crate::pipeline::extract::PaddleStructuredBlock) -> bool {
    paddle_block_label(block).eq_ignore_ascii_case("formula_number")
}

fn parse_paddle_formula_number(value: &str) -> Option<String> {
    let mut value = value.trim();
    while value.len() >= 2 && value.starts_with('$') && value.ends_with('$') {
        value = value[1..value.len() - 1].trim();
    }
    let wrapped_in_math_delimiters = (value.starts_with(r"\(") && value.ends_with(r"\)"))
        || (value.starts_with(r"\[") && value.ends_with(r"\]"));
    if wrapped_in_math_delimiters && value.len() >= 4 {
        value = value[2..value.len() - 2].trim();
    }
    if let Some(capture) = Regex::new(r"^\\tag\*?\{([^{}]{1,40})\}$")
        .unwrap()
        .captures(value)
    {
        value = capture.get(1)?.as_str().trim();
    }
    if ((value.starts_with('(') && value.ends_with(')'))
        || (value.starts_with('[') && value.ends_with(']')))
        && value.len() >= 2
    {
        value = value[1..value.len() - 1].trim();
    }
    if Regex::new(r"(?i)^[a-z0-9]+(?:[.:-][a-z0-9]+)*$")
        .unwrap()
        .is_match(value)
    {
        Some(value.to_string())
    } else {
        None
    }
}

fn paddle_formula_number(
    block: &crate::pipeline::extract::PaddleStructuredBlock,
) -> Option<String> {
    if !is_paddle_formula_number(block) {
        return None;
    }
    parse_paddle_formula_number(&block.text)
        .or_else(|| parse_paddle_formula_number(&block.markdown))
}

#[derive(Debug, Clone, Copy)]
struct PaddleBlockGeometry {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl PaddleBlockGeometry {
    fn width(self) -> f64 {
        self.right - self.left
    }

    fn height(self) -> f64 {
        self.bottom - self.top
    }

    fn center_x(self) -> f64 {
        (self.left + self.right) / 2.0
    }

    fn center_y(self) -> f64 {
        (self.top + self.bottom) / 2.0
    }
}

fn paddle_block_geometry(
    block: &crate::pipeline::extract::PaddleStructuredBlock,
) -> Option<PaddleBlockGeometry> {
    let coordinates = if block.bbox.len() >= 4 {
        Some((block.bbox[0], block.bbox[1], block.bbox[2], block.bbox[3]))
    } else if block.polygon.len() >= 3 {
        let mut xs = block
            .polygon
            .iter()
            .filter_map(|point| point.first().copied());
        let mut ys = block
            .polygon
            .iter()
            .filter_map(|point| point.get(1).copied());
        let first_x = xs.next()?;
        let first_y = ys.next()?;
        Some((
            xs.clone().fold(first_x, f64::min),
            ys.clone().fold(first_y, f64::min),
            xs.fold(first_x, f64::max),
            ys.fold(first_y, f64::max),
        ))
    } else {
        None
    }?;
    let geometry = PaddleBlockGeometry {
        left: coordinates.0,
        top: coordinates.1,
        right: coordinates.2,
        bottom: coordinates.3,
    };
    (coordinates.0.is_finite()
        && coordinates.1.is_finite()
        && coordinates.2.is_finite()
        && coordinates.3.is_finite()
        && geometry.width() > 0.0
        && geometry.height() > 0.0)
        .then_some(geometry)
}

fn paddle_formula_match_score(
    equation: &crate::pipeline::extract::PaddleStructuredBlock,
    number: &crate::pipeline::extract::PaddleStructuredBlock,
    page_width: f64,
) -> Option<f64> {
    if number.confidence.is_some_and(|confidence| confidence < 0.5) {
        return None;
    }
    let equation = paddle_block_geometry(equation)?;
    let number = paddle_block_geometry(number)?;
    if number.width() > page_width * 0.22 {
        return None;
    }

    let overlap = (equation.bottom.min(number.bottom) - equation.top.max(number.top)).max(0.0);
    let overlap_ratio = overlap / equation.height().min(number.height()).max(1.0);
    let vertical_distance = (equation.center_y() - number.center_y()).abs();
    if overlap_ratio < 0.2
        && vertical_distance > (equation.height().max(number.height()) * 0.75).max(12.0)
    {
        return None;
    }

    let center_separation = (equation.center_x() - number.center_x()).abs();
    if center_separation < (equation.width() * 0.2).max(page_width * 0.04) {
        return None;
    }
    let horizontal_gap = if number.left >= equation.right {
        number.left - equation.right
    } else if equation.left >= number.right {
        equation.left - number.right
    } else {
        0.0
    };
    if horizontal_gap > page_width * 0.4 {
        return None;
    }
    let edge_distance = (number.center_x() - equation.left)
        .abs()
        .min((number.center_x() - equation.right).abs());
    Some(
        vertical_distance / equation.height().max(number.height()).max(1.0)
            + 3.0 * horizontal_gap / page_width
            + 0.5 * edge_distance / page_width,
    )
}

fn unambiguous_best(mut candidates: Vec<(usize, f64)>) -> Option<(usize, f64)> {
    const MIN_SCORE_SEPARATION: f64 = 0.12;
    candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
    let best = *candidates.first()?;
    if candidates
        .get(1)
        .is_some_and(|second| second.1 - best.1 < MIN_SCORE_SEPARATION)
    {
        None
    } else {
        Some(best)
    }
}

/// Match formula-number regions to equations using page geometry. A match is
/// accepted only when the equation and number are mutual, unambiguous nearest
/// candidates; otherwise the number remains an ordinary visible text block.
fn match_paddle_formula_numbers(
    page: &crate::pipeline::extract::PaddleStructuredPage,
) -> HashMap<usize, usize> {
    let equations = page
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| paddle_node_kind(block) == "equation")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let numbers = page
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| paddle_formula_number(block).is_some())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let inferred_width = page
        .blocks
        .iter()
        .filter_map(paddle_block_geometry)
        .map(|geometry| geometry.right)
        .fold(0.0, f64::max);
    let page_width = f64::from(page.width.unwrap_or(0))
        .max(inferred_width)
        .max(1.0);

    let mut candidates = Vec::new();
    for &number_index in &numbers {
        for &equation_index in &equations {
            if let Some(score) = paddle_formula_match_score(
                &page.blocks[equation_index],
                &page.blocks[number_index],
                page_width,
            ) {
                candidates.push((equation_index, number_index, score));
            }
        }
    }
    let best_by_number = numbers
        .iter()
        .filter_map(|&number_index| {
            unambiguous_best(
                candidates
                    .iter()
                    .filter(|(_, candidate, _)| *candidate == number_index)
                    .map(|(equation, _, score)| (*equation, *score))
                    .collect(),
            )
            .map(|best| (number_index, best))
        })
        .collect::<HashMap<_, _>>();
    let best_by_equation = equations
        .iter()
        .filter_map(|&equation_index| {
            unambiguous_best(
                candidates
                    .iter()
                    .filter(|(candidate, _, _)| *candidate == equation_index)
                    .map(|(_, number, score)| (*number, *score))
                    .collect(),
            )
            .map(|best| (equation_index, best))
        })
        .collect::<HashMap<_, _>>();

    best_by_equation
        .into_iter()
        .filter_map(|(equation, (number, _))| {
            best_by_number
                .get(&number)
                .is_some_and(|(candidate, _)| *candidate == equation)
                .then_some((equation, number))
        })
        .collect()
}

fn paddle_block_representation(
    block: &crate::pipeline::extract::PaddleStructuredBlock,
    structure: &crate::pipeline::extract::PaddleStructure,
) -> DocumentRepresentation {
    DocumentRepresentation {
        format: "paddle_block".to_string(),
        content: serde_json::json!({
            "block_id": block.block_id,
            "role": block.role,
            "block_label": block.block_label,
            "markdown": block.markdown,
            "boundary": block.boundary,
            "note_marker": block.note_marker,
            "order": block.order,
            "bbox": block.bbox,
            "polygon": block.polygon,
            "confidence": block.confidence,
            "asset_files": block.asset_files,
            "raw": block.raw,
            "parser": structure.parser,
            "parser_version": structure.parser_version,
            "parser_settings": structure.settings,
        }),
    }
}

fn paddle_node_label(kind: &str, page: u32) -> String {
    match kind {
        "footnote" => format!("Footnote on page {page}"),
        "possible_footnote" => format!("Possible footnote on page {page}"),
        "page_header" => format!("Page {page} header"),
        "page_footer" => format!("Page {page} footer"),
        "section" => format!("Section block on page {page}"),
        "equation" => format!("Equation block on page {page}"),
        "table" => format!("Table block on page {page}"),
        "figure" => format!("Figure block on page {page}"),
        "caption" => format!("Caption block on page {page}"),
        _ => format!("PaddleOCR-VL block on page {page}"),
    }
}

fn paddle_heading_level(block: &crate::pipeline::extract::PaddleStructuredBlock) -> Option<usize> {
    if let Some(capture) = Regex::new(r"^(#{1,6})\s+")
        .unwrap()
        .captures(block.markdown.trim_start())
    {
        return capture.get(1).map(|marker| marker.as_str().len());
    }
    let label = if block.block_label.is_empty() {
        block.role.as_str()
    } else {
        block.block_label.as_str()
    };
    match label.to_ascii_lowercase().as_str() {
        "doc_title" | "document_title" => Some(1),
        "paragraph_title" | "section_title" | "abstract_title" | "reference_title" => Some(2),
        _ => None,
    }
}

fn paddle_block_assets(
    bundle: &DocumentBundle,
    block: &crate::pipeline::extract::PaddleStructuredBlock,
    page: u32,
    kind: &str,
) -> Vec<String> {
    let names: HashSet<&str> = block
        .asset_files
        .iter()
        .filter_map(|path| Path::new(path).file_name().and_then(|name| name.to_str()))
        .collect();
    let mut asset_ids: Vec<String> = bundle
        .assets
        .iter()
        .filter(|asset| {
            names.contains(asset.label.as_str())
                || names.iter().any(|name| asset.rel_path.ends_with(name))
        })
        .map(|asset| asset.id.clone())
        .collect();
    if asset_ids.is_empty() && matches!(kind, "figure" | "table") {
        if let Some(asset) = bundle
            .assets
            .iter()
            .find(|asset| asset.kind == "page" && asset.page == Some(page))
        {
            asset_ids.push(asset.id.clone());
        }
    }
    asset_ids.sort();
    asset_ids.dedup();
    asset_ids
}

pub(super) fn add_paddle_structured_nodes(
    bundle: &mut DocumentBundle,
    structure: &crate::pipeline::extract::PaddleStructure,
) {
    let mut heading_stack: [Option<String>; 6] = std::array::from_fn(|_| None);
    for page in &structure.pages {
        let formula_matches = match_paddle_formula_numbers(page);
        let matched_numbers = formula_matches.values().copied().collect::<HashSet<_>>();
        for (block_index, block) in page.blocks.iter().enumerate() {
            if matched_numbers.contains(&block_index) {
                // The matched equation owns the number and retains this block
                // as an additional lossless representation.
                continue;
            }
            let kind = paddle_node_kind(block);
            let asset_ids = paddle_block_assets(bundle, block, page.number, kind);
            let mut representations = vec![paddle_block_representation(block, structure)];
            let number_block = formula_matches
                .get(&block_index)
                .and_then(|index| page.blocks.get(*index));
            let number = number_block.and_then(paddle_formula_number);
            if let Some(number_block) = number_block {
                representations.push(paddle_block_representation(number_block, structure));
            }
            let mut created = node(
                bundle,
                kind,
                Some(page.number),
                Some(paddle_node_label(kind, page.number)),
                number.or_else(|| block.note_marker.clone()),
                block.text.clone(),
                asset_ids,
                representations,
                if structure.parser == "paddleocr-vl-full" {
                    "paddleocr-vl-full"
                } else {
                    "paddleocr-vl"
                },
            );
            created.provenance.confidence = block.confidence.or(match block.role.as_str() {
                "possible_footnote" => Some(0.6),
                "page_header" | "page_footer" => Some(0.8),
                _ => None,
            });
            if !matches!(kind, "page_header" | "page_footer") {
                if let Some(level) = paddle_heading_level(block) {
                    created.parent_id = heading_stack[..level.saturating_sub(1)]
                        .iter()
                        .rev()
                        .find_map(|value| value.clone());
                    heading_stack[level - 1] = Some(created.id.clone());
                    for slot in &mut heading_stack[level..] {
                        *slot = None;
                    }
                } else {
                    created.parent_id = heading_stack.iter().rev().find_map(|value| value.clone());
                }
            };
            bundle.nodes.push(created);
        }
    }
}
