use crate::models::{Finding, FindingSet};

/// Deterministic human presentation of a canonical findings product. Findings
/// are grouped under their category in order of first appearance, so
/// centrality-ordered arrays never repeat section headers; within a category,
/// array order carries importance. Display numbers follow the rendered order
/// and are deliberately not identities.
pub fn render_markdown(findings: &FindingSet) -> String {
    if findings.findings.is_empty() {
        return "No findings.".to_string();
    }
    fn category_of(finding: &Finding) -> &str {
        let category = finding.category.trim();
        if category.is_empty() {
            "Findings"
        } else {
            category
        }
    }
    let mut categories: Vec<&str> = Vec::new();
    for finding in &findings.findings {
        let category = category_of(finding);
        if !categories.contains(&category) {
            categories.push(category);
        }
    }
    let mut markdown = String::new();
    let mut number = 0usize;
    for category in categories {
        if !markdown.is_empty() {
            markdown.push('\n');
        }
        markdown.push_str("## ");
        markdown.push_str(category);
        markdown.push_str("\n\n");
        for finding in findings
            .findings
            .iter()
            .filter(|finding| category_of(finding) == category)
        {
            number += 1;
            markdown.push_str(&format!("**#{}. {}**\n\n", number, finding.title.trim()));
            markdown.push_str(finding.body.trim());
            markdown.push_str("\n\n");
        }
    }
    markdown.trim().to_string()
}
