use super::*;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FindingPackage {
    pub version: u32,
    pub run_id: String,
    pub source_step_id: String,
    pub findings: Vec<crate::models::Finding>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportComment {
    pub number: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResponseDecision {
    pub number: String,
    pub category: String,
    pub severity: String,
    pub disposition: String,
    pub intended_response: String,
    pub task_id: Option<String>,
    pub manuscript_revision_id: Option<String>,
    pub application_id: Option<String>,
    pub execution_id: Option<String>,
    pub evidence_anchor_ids: Vec<String>,
    pub draft: String,
    pub rationale: String,
    pub disputed_premise: String,
    pub counterargument: String,
    pub resolving_check: String,
    pub reports_analysis_added: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseRecord {
    pub source: Value,
    pub decision: ResponseDecision,
    pub flags: Vec<String>,
}
fn initial(number: String) -> ResponseDecision {
    ResponseDecision {
        number,
        category: "unresolved_objection".into(),
        severity: "unspecified".into(),
        disposition: "open".into(),
        ..Default::default()
    }
}
pub fn preview_findings(package: &FindingPackage) -> WorkbenchResult<Value> {
    if package.version != 1 || package.findings.len() > 1000 {
        return Err(WorkbenchError::invalid(
            "Expected a findings exchange v1 package with at most 1000 findings",
        ));
    }
    bounded(&package.run_id, 200)?;
    bounded(&package.source_step_id, 200)?;
    let encoded = serde_json::to_vec(package).map_err(err)?;
    if encoded.len() > 4 * 1024 * 1024 {
        return Err(WorkbenchError::invalid("Finding package exceeds 4 MiB"));
    }
    let mut ids = BTreeSet::new();
    for f in &package.findings {
        bounded(&f.id, 200)?;
        if !ids.insert(f.id.clone()) {
            return Err(WorkbenchError::invalid("Duplicate finding IDs in package"));
        }
        bounded(&f.title, 2000)?;
        if f.body.len() > 100_000 {
            return Err(WorkbenchError::invalid("Finding text is too large"));
        }
    }
    Ok(
        json!({"packageHash":hash(&encoded),"package":package,"notice":"Imported findings are review input. Their verification labels do not become Workspace confirmation."}),
    )
}
pub(super) fn import_findings(
    store: &Store,
    ws: &str,
    package: FindingPackage,
    selected: &[String],
) -> WorkbenchResult<Value> {
    let preview = preview_findings(&package)?;
    if selected.is_empty()
        || selected.len() > 1000
        || selected
            .iter()
            .any(|id| !package.findings.iter().any(|f| &f.id == id))
    {
        return Err(WorkbenchError::invalid("Select findings from the preview"));
    }
    let snapshot = blob(
        store,
        ws,
        &serde_json::to_vec(&package).map_err(err)?,
        "finding_exchange",
    )?;
    let mut imported = Vec::new();
    for f in package.findings.iter().filter(|f| selected.contains(&f.id)) {
        let object_id = format!(
            "response_{}",
            hash(
                format!(
                    "workflow\0{}\0{}\0{}",
                    package.run_id, package.source_step_id, f.id
                )
                .as_bytes()
            )
        );
        let source = json!({"kind":"workflow","runId":package.run_id,"sourceStepId":package.source_step_id,"findingId":f.id,"snapshotHash":snapshot,"finding":f,"packageHash":preview["packageHash"]});
        let old = record(store, ws, &object_id, "response").ok();
        if old.as_ref().is_some_and(|r| r.body["source"] == source) {
            imported.push(old.unwrap());
            continue;
        }
        let decision = if let Some(old) = &old {
            decode::<ResponseRecord>(old)?.decision
        } else {
            initial(if f.rank > 0 {
                f.rank.to_string()
            } else {
                f.id.clone()
            })
        };
        let flags = response_flags(store, ws, &decision)?;
        imported.push(put(
            store,
            ws,
            &object_id,
            "response",
            old.map_or(0, |r| r.revision),
            &ResponseRecord {
                source,
                decision,
                flags,
            },
        )?);
    }
    Ok(json!(imported))
}
pub fn preview_report(
    store: &Store,
    ws: &str,
    revision: &str,
) -> WorkbenchResult<Vec<ReportComment>> {
    let text = exact_text(store, ws, revision)?;
    let mut comments = Vec::new();
    let mut start = 0;
    for paragraph in text.split("\n\n") {
        let end = start + paragraph.len();
        if !paragraph.trim().is_empty() {
            comments.push(ReportComment {
                number: (comments.len() + 1).to_string(),
                start,
                end,
                text: paragraph.into(),
            });
        }
        start = end + 2;
        if comments.len() >= 500 {
            break;
        }
    }
    Ok(comments)
}
pub(super) fn import_report(
    store: &Store,
    ws: &str,
    revision: &str,
    comments: Vec<ReportComment>,
) -> WorkbenchResult<Value> {
    let original = exact_text(store, ws, revision)?;
    if comments.is_empty() || comments.len() > 500 {
        return Err(WorkbenchError::invalid("Select 1–500 report comments"));
    }
    let mut result = Vec::new();
    let mut numbers = BTreeSet::new();
    for c in comments {
        bounded(&c.number, 100)?;
        if !numbers.insert(c.number.clone())
            || original.get(c.start..c.end) != Some(c.text.as_str())
            || c.text.trim().is_empty()
        {
            return Err(WorkbenchError::invalid(
                "Comment locators must preserve exact report text and unique numbering",
            ));
        }
        let object_id = format!(
            "response_{}",
            hash(format!("report\0{revision}\0{}\0{}", c.start, c.end).as_bytes())
        );
        if let Ok(old) = record(store, ws, &object_id, "response") {
            result.push(old);
            continue;
        }
        result.push(put(
            store,
            ws,
            &object_id,
            "response",
            0,
            &ResponseRecord {
                source: json!({"kind":"report","revisionId":revision,"comment":c}),
                decision: initial(c.number),
                flags: vec![],
            },
        )?);
    }
    Ok(json!(result))
}
pub fn response_flags(store: &Store, ws: &str, d: &ResponseDecision) -> WorkbenchResult<Vec<String>> {
    let mut flags = Vec::new();
    let analysis=d.reports_analysis_added||regex::Regex::new(r"(?i)\b(added|ran|reran|performed|conducted)\b.{0,70}\b(analysis|regression|experiment|robustness|simulation|estimation)\b").expect("response assertion").is_match(&d.draft);
    if analysis {
        if !d
            .execution_id
            .as_ref()
            .and_then(|id| execution(store, ws, id).ok())
            .is_some_and(|e| e.outcome == "completed")
        {
            flags.push("Analysis-added statement lacks a completed execution".into());
        }
        if !d
            .application_id
            .as_ref()
            .and_then(|id| record(store, ws, id, "application").ok())
            .is_some_and(|r| r.body["state"] == "applied")
        {
            flags.push("Analysis-added statement lacks an accepted manuscript change".into());
        }
        if d.manuscript_revision_id.is_none() {
            flags.push("Analysis-added statement lacks an exact manuscript revision".into());
        }
    }
    if d.disposition == "addressed" && d.draft.trim().is_empty() {
        flags.push("Addressed comment has no response draft".into());
    }
    if matches!(d.disposition.as_str(), "deferred" | "rejected") && d.rationale.trim().is_empty() {
        flags.push("Explain the decision to defer or reject this comment".into());
    }
    if d.draft.trim().is_empty() {
        flags.push("Response draft is empty".into());
    }
    Ok(flags)
}
pub(super) fn save_response(
    store: &Store,
    ws: &str,
    object_id: &str,
    expected: i64,
    d: ResponseDecision,
) -> WorkbenchResult<ProjectRecord> {
    let old: ResponseRecord = decode(&record(store, ws, object_id, "response")?)?;
    bounded(&d.number, 100)?;
    if !matches!(
        d.category.as_str(),
        "observed_error"
            | "unresolved_objection"
            | "missing_robustness"
            | "omitted_source"
            | "unclear_exposition"
            | "optional_extension"
    ) || !matches!(
        d.disposition.as_str(),
        "open" | "investigating" | "deferred" | "rejected" | "addressed"
    ) {
        return Err(WorkbenchError::invalid(
            "Unknown response category or disposition",
        ));
    }
    if d.evidence_anchor_ids.len() > 100 || serde_json::to_vec(&d).map_err(err)?.len() > 256 * 1024
    {
        return Err(WorkbenchError::invalid("Response is too large"));
    }
    for a in &d.evidence_anchor_ids {
        record(store, ws, a, "anchor")?;
    }
    if let Some(id) = &d.task_id {
        record(store, ws, id, "task")?;
    }
    if let Some(id) = &d.application_id {
        record(store, ws, id, "application")?;
    }
    if let Some(id) = &d.manuscript_revision_id {
        read_document(store, ws, id, 0, None)?;
    }
    if let Some(id) = &d.execution_id {
        execution(store, ws, id)?;
    }
    let flags = response_flags(store, ws, &d)?;
    put(
        store,
        ws,
        object_id,
        "response",
        expected,
        &ResponseRecord {
            source: old.source,
            decision: d,
            flags,
        },
    )
}
pub fn export_responses(
    store: &Store,
    ws: &str,
    selected: &[String],
    format: &str,
) -> WorkbenchResult<Value> {
    if selected.is_empty() || selected.len() > 500 || !matches!(format, "markdown" | "latex") {
        return Err(WorkbenchError::invalid(
            "Select comments and Markdown or LaTeX",
        ));
    }
    let mut output = String::new();
    let mut warnings = Vec::new();
    let mut numbers = BTreeSet::new();
    if format == "latex" {
        output.push_str("\\documentclass{article}\n\\usepackage[T1]{fontenc}\n\\begin{document}\n\\section*{Response to reviewers}\n");
    } else {
        output.push_str("# Response to reviewers\n\n");
    }
    for object_id in selected {
        let r: ResponseRecord = decode(&record(store, ws, object_id, "response")?)?;
        let d = &r.decision;
        if !numbers.insert(d.number.clone()) {
            warnings.push(format!("Duplicate comment number {}", d.number));
        }
        warnings.extend(
            response_flags(store, ws, d)?
                .into_iter()
                .map(|f| format!("Comment {}: {f}", d.number)),
        );
        let original = r.source["comment"]["text"]
            .as_str()
            .or_else(|| r.source["finding"]["body"].as_str())
            .unwrap_or("");
        let reference = format!(
            "Disposition: {}. Manuscript: {}. Execution: {}. Accepted change: {}.",
            d.disposition,
            d.manuscript_revision_id.as_deref().unwrap_or("unlinked"),
            d.execution_id.as_deref().unwrap_or("unlinked"),
            d.application_id.as_deref().unwrap_or("unlinked")
        );
        if format == "latex" {
            output.push_str(&format!("\\subsection*{{Comment {}}}\n\\begin{{quote}}{}\\end{{quote}}\n{}\n\n\\small {}\\normalsize\n",tex_escape(&d.number),tex_escape(original),tex_escape(&d.draft),tex_escape(&reference)));
        } else {
            output.push_str(&format!(
                "## Comment {}\n\n{}\n\n{}\n\n{}\n\n",
                d.number,
                original
                    .lines()
                    .map(|l| format!("> {l}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                d.draft,
                reference
            ));
        }
    }
    if format == "latex" {
        output.push_str("\\end{document}\n");
    }
    Ok(
        json!({"text":output,"warnings":warnings,"draft":!warnings.is_empty(),"format":format,"assessment":"Checks cover declared links and common analysis-added wording, not every semantic assertion."}),
    )
}
pub(super) fn tex_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\\' => "\\textbackslash{}".into(),
            '{' | '}' | '$' | '&' | '#' | '%' | '_' => format!("\\{c}"),
            '~' => "\\textasciitilde{}".into(),
            '^' => "\\textasciicircum{}".into(),
            c => c.to_string(),
        })
        .collect()
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusReviewRequest {
    pub workspace_id: String,
    pub anchor_ids: Vec<String>,
    pub dependency_anchor_ids: Vec<String>,
    pub response_ids: Vec<String>,
}
pub fn focus_review(
    store: &Store,
    r: FocusReviewRequest,
) -> WorkbenchResult<super::super::super::release::ReviewHandoff> {
    scope(store, &r.workspace_id)?;
    if r.anchor_ids.is_empty()
        || r.anchor_ids.len() + r.dependency_anchor_ids.len() > 100
        || r.response_ids.len() > 100
    {
        return Err(WorkbenchError::invalid(
            "Choose 1–100 exact passages and their declared dependencies",
        ));
    }
    let mut text=String::from("# Focused re-review\n\nCoverage is limited to the selected passages, comments and declared dependencies below. Omitted manuscript sections have not been reviewed.\n\n");
    let mut manifest = Vec::new();
    for (role, ids) in [
        ("changed passage", &r.anchor_ids),
        ("declared dependency", &r.dependency_anchor_ids),
    ] {
        for aid in ids {
            let a = record(store, &r.workspace_id, aid, "anchor")?;
            text.push_str(&format!(
                "## {role}: {aid}\n\n{}\n\n",
                serde_json::to_string_pretty(&a.body).map_err(err)?
            ));
            manifest.push(json!({"role":role,"anchor":a}));
        }
    }
    for rid in &r.response_ids {
        let response = record(store, &r.workspace_id, rid, "response")?;
        text.push_str(&format!(
            "## Review comment\n\n{}\n\n",
            serde_json::to_string_pretty(&response.body).map_err(err)?
        ));
    }
    let digest = blob(store, &r.workspace_id, text.as_bytes(), "focused_review")?;
    // Neutral Markdown snapshot, then the ordinary immutable handoff/Workflow preview.
    let path = store.root_path().join("blobs").join(format!("{digest}.md"));
    if !path.exists() {
        fs::write(&path, text).map_err(err)?;
    }
    let paper = research::import_paper(
        store,
        research::ImportPaperRequest {
            workspace_id: r.workspace_id.clone(),
            paper_id: None,
            title: "Focused re-review coverage".into(),
            role: "other".into(),
            path: path.to_string_lossy().into_owned(),
            operation_id: id("focus_paper")?,
        },
    )?;
    super::super::super::release::prepare_review_handoff(
        store,
        super::super::super::release::PrepareReviewHandoffRequest {
            workspace_id: r.workspace_id,
            session_id: None,
            paper_id: paper.paper.id,
            metadata: json!({"coverage":manifest,"responseIds":r.response_ids,"fullManuscriptReview":false}),
            operation_id: id("focus_handoff")?,
        },
    )
}
