//! Lightweight theory records, typed check receipts, and research directions.
//!
//! These are structured project records, not a proof assistant or notebook
//! runtime. The host labels evidence class and scope; the researcher owns the
//! status of every note. A numerical check can never be recorded as a general
//! proof, a proof sketch keeps its unresolved steps, and a rejected approach
//! keeps its assumptions and reason so a later session can find it.
use super::*;

pub const THEORY_KINDS: [&str; 8] = [
    "assumption",
    "conjecture",
    "proposition",
    "derivation",
    "proof_sketch",
    "unresolved_step",
    "counterexample",
    "rejected_approach",
];
pub const THEORY_STATUSES: [&str; 4] = ["open", "supported", "refuted", "abandoned"];
pub const CHECK_METHODS: [&str; 6] = [
    "analytical_argument",
    "symbolic_identity",
    "numerical_verification",
    "numerical_counterexample",
    "heuristic",
    "model_assessment",
];
pub const CHECK_OUTCOMES: [&str; 3] = ["passed", "failed", "inconclusive"];
pub const DIRECTION_STATUSES: [&str; 4] = ["idea", "active", "converted", "dropped"];
const MAX_LINKS: usize = 50;
const MAX_BODY: usize = 128 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Promotion {
    pub checkpoint_id: String,
    pub path: String,
    pub note_revision: i64,
    pub promoted_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TheoryNote {
    pub kind: String,
    pub title: String,
    pub statement: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub assumption_ids: Vec<String>,
    #[serde(default)]
    pub anchor_ids: Vec<String>,
    #[serde(default)]
    pub related_ids: Vec<String>,
    #[serde(default)]
    pub unresolved_steps: Vec<String>,
    pub status: String,
    #[serde(default)]
    pub rejection_reason: String,
    #[serde(default = "manual")]
    pub origin: String,
    /// Host-owned; client-supplied values are replaced by the stored list.
    #[serde(default)]
    pub promotions: Vec<Promotion>,
}
fn manual() -> String {
    "manual".into()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TheoryCheck {
    pub theory_id: String,
    pub method: String,
    pub outcome: String,
    pub summary: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub tolerance: Option<f64>,
    #[serde(default)]
    pub precision: Option<u32>,
    #[serde(default)]
    pub execution_id: Option<String>,
    #[serde(default)]
    pub recipe_run_id: Option<String>,
    #[serde(default)]
    pub anchor_ids: Vec<String>,
    /// The author asserts this check establishes the statement in general.
    #[serde(default)]
    pub claims_generality: bool,
    #[serde(default = "manual")]
    pub origin: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TheoryCheckRecord {
    pub check: TheoryCheck,
    /// Host-owned evidence scope: `general`, `identity_within_domain`,
    /// `instances_only`, `refuting_instance`, `heuristic`, or `model_assessment`.
    pub scope: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResearchDirection {
    pub question: String,
    #[serde(default)]
    pub mechanism: String,
    #[serde(default)]
    pub closest_known_work: String,
    #[serde(default)]
    pub minimal_model_or_data: String,
    #[serde(default)]
    pub first_discriminating_test: String,
    #[serde(default)]
    pub likely_failure_mode: String,
    #[serde(default)]
    pub next_action: String,
    pub status: String,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub theory_ids: Vec<String>,
    #[serde(default)]
    pub drop_reason: String,
}

fn optional(value: &str, max: usize) -> WorkbenchResult<()> {
    if value.len() > max || value.contains('\0') {
        return Err(WorkbenchError::invalid("Text exceeds its limit"));
    }
    Ok(())
}
fn list(values: &[String], max_each: usize) -> WorkbenchResult<()> {
    if values.len() > MAX_LINKS {
        return Err(WorkbenchError::invalid(
            "At most 50 entries per list on a theory record",
        ));
    }
    values.iter().try_for_each(|v| bounded(v, max_each))
}
fn linked(store: &Store, ws: &str, ids: &[String], kind: &str) -> WorkbenchResult<()> {
    if ids.len() > MAX_LINKS {
        return Err(WorkbenchError::invalid("At most 50 links per record"));
    }
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(WorkbenchError::invalid("Duplicate link on a record"));
        }
        record(store, ws, id, kind)?;
    }
    Ok(())
}
fn origin_ok(origin: &str) -> WorkbenchResult<()> {
    if !["manual", "proposed"].contains(&origin) {
        return Err(WorkbenchError::invalid("Origin must be manual or proposed"));
    }
    Ok(())
}

pub(super) fn save_theory(
    store: &Store,
    ws: &str,
    object_id: Option<&str>,
    expected: i64,
    mut note: TheoryNote,
) -> WorkbenchResult<ProjectRecord> {
    if !THEORY_KINDS.contains(&note.kind.as_str()) {
        return Err(WorkbenchError::invalid("Unknown theory note kind"));
    }
    if !THEORY_STATUSES.contains(&note.status.as_str()) {
        return Err(WorkbenchError::invalid("Unknown theory note status"));
    }
    origin_ok(&note.origin)?;
    bounded(&note.title, 300)?;
    bounded(&note.statement, 16_000)?;
    optional(&note.body, MAX_BODY)?;
    optional(&note.rejection_reason, 8_000)?;
    list(&note.assumptions, 2_000)?;
    list(&note.unresolved_steps, 2_000)?;
    linked(store, ws, &note.anchor_ids, "anchor")?;
    let id = new_or_id(object_id)?;
    if note.related_ids.iter().any(|r| r == &id) || note.assumption_ids.iter().any(|r| r == &id) {
        return Err(WorkbenchError::invalid(
            "A theory note cannot link to itself",
        ));
    }
    linked(store, ws, &note.related_ids, "theory")?;
    linked(store, ws, &note.assumption_ids, "theory")?;
    for assumption in &note.assumption_ids {
        if decode::<TheoryNote>(&record(store, ws, assumption, "theory")?)?.kind != "assumption" {
            return Err(WorkbenchError::invalid(
                "Assumption links must point at assumption notes",
            ));
        }
    }
    if note.kind == "rejected_approach" && note.status != "abandoned" {
        return Err(WorkbenchError::invalid(
            "A rejected approach is recorded with status abandoned",
        ));
    }
    if note.status == "abandoned" && note.rejection_reason.trim().is_empty() {
        return Err(WorkbenchError::invalid(
            "Record why this approach was abandoned so a later session can find it",
        ));
    }
    if note.status == "supported"
        && matches!(
            note.kind.as_str(),
            "derivation" | "proof_sketch" | "proposition" | "conjecture"
        )
        && !note.unresolved_steps.is_empty()
    {
        return Err(WorkbenchError::invalid(
            "A derivation with unresolved steps cannot be recorded as supported",
        ));
    }
    if note.origin == "proposed" && note.status != "open" {
        return Err(WorkbenchError::invalid(
            "A proposed note stays open until the researcher reviews it",
        ));
    }
    note.promotions = match object_id {
        Some(existing) => decode::<TheoryNote>(&record(store, ws, existing, "theory")?)?.promotions,
        None => Vec::new(),
    };
    put(store, ws, &id, "theory", expected, &note)
}

fn check_scope(check: &TheoryCheck) -> WorkbenchResult<(String, String)> {
    let numerical = matches!(
        check.method.as_str(),
        "numerical_verification" | "numerical_counterexample"
    );
    if numerical {
        bounded(&check.domain, 4_000).map_err(|_| {
            WorkbenchError::invalid(
                "Numerical checks must state the domain of instances that were tested",
            )
        })?;
        if check.claims_generality {
            return Err(WorkbenchError::invalid(
                "A numerical check covers tested instances only and cannot be labeled a general proof",
            ));
        }
    }
    if matches!(check.method.as_str(), "heuristic" | "model_assessment") && check.claims_generality
    {
        return Err(WorkbenchError::invalid(
            "Heuristics and model assessments cannot be labeled as establishing a statement",
        ));
    }
    if let Some(t) = check.tolerance {
        if !t.is_finite() || t < 0.0 {
            return Err(WorkbenchError::invalid(
                "Tolerance must be a finite non-negative number",
            ));
        }
    }
    if check.precision.is_some_and(|p| p > 30) {
        return Err(WorkbenchError::invalid("Precision is at most 30 digits"));
    }
    let passed = check.outcome == "passed";
    Ok(match check.method.as_str() {
        "analytical_argument" if check.claims_generality && passed => (
            "general".into(),
            "Researcher-recorded analytical argument; not machine-verified.".into(),
        ),
        "analytical_argument" => (
            "argument".into(),
            "Analytical argument recorded without a claim of generality.".into(),
        ),
        "symbolic_identity" if check.claims_generality && passed => (
            "identity_within_domain".into(),
            format!(
                "Symbolic identity holds on the stated domain{}.",
                if check.domain.trim().is_empty() {
                    " (domain unstated)"
                } else {
                    ""
                }
            ),
        ),
        "symbolic_identity" => (
            "identity_check".into(),
            "Symbolic check recorded; scope not asserted.".into(),
        ),
        "numerical_verification" => (
            "instances_only".into(),
            "Numerical verification on tested instances only; this is not a general proof.".into(),
        ),
        "numerical_counterexample" if passed => (
            "refuting_instance".into(),
            "A confirmed numerical counterexample refutes the general statement as stated.".into(),
        ),
        "numerical_counterexample" => (
            "instances_only".into(),
            "No confirmed counterexample; tested instances only.".into(),
        ),
        "heuristic" => (
            "heuristic".into(),
            "Heuristic reasoning; neither proof nor verification.".into(),
        ),
        _ => (
            "model_assessment".into(),
            "Model assessment; not researcher confirmation or verification.".into(),
        ),
    })
}

pub(super) fn save_check(
    store: &Store,
    ws: &str,
    object_id: Option<&str>,
    expected: i64,
    check: TheoryCheck,
) -> WorkbenchResult<ProjectRecord> {
    if !CHECK_METHODS.contains(&check.method.as_str()) {
        return Err(WorkbenchError::invalid("Unknown check method"));
    }
    if !CHECK_OUTCOMES.contains(&check.outcome.as_str()) {
        return Err(WorkbenchError::invalid("Unknown check outcome"));
    }
    origin_ok(&check.origin)?;
    bounded(&check.summary, 16_000)?;
    optional(&check.domain, 4_000)?;
    record(store, ws, &check.theory_id, "theory")?;
    linked(store, ws, &check.anchor_ids, "anchor")?;
    if let Some(execution_id) = &check.execution_id {
        if execution(store, ws, execution_id)?.outcome != "completed" {
            return Err(WorkbenchError::invalid(
                "Link a completed execution from this Workspace",
            ));
        }
    }
    if let Some(run) = &check.recipe_run_id {
        valid_id(run)?;
        let exists: bool = store
            .connection()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM recipe_runs WHERE id=?1 AND workspace_id=?2)",
                params![run, ws],
                |r| r.get(0),
            )
            .map_err(err)?;
        if !exists {
            return Err(WorkbenchError::invalid(
                "Recipe run is not in this Workspace",
            ));
        }
    }
    let (scope, label) = check_scope(&check)?;
    put(
        store,
        ws,
        &new_or_id(object_id)?,
        "check",
        expected,
        &TheoryCheckRecord {
            check,
            scope,
            label,
        },
    )
}

pub(super) fn save_direction(
    store: &Store,
    ws: &str,
    object_id: Option<&str>,
    expected: i64,
    d: ResearchDirection,
) -> WorkbenchResult<ProjectRecord> {
    if !DIRECTION_STATUSES.contains(&d.status.as_str()) {
        return Err(WorkbenchError::invalid("Unknown research direction status"));
    }
    bounded(&d.question, 4_000)?;
    for field in [
        &d.mechanism,
        &d.closest_known_work,
        &d.minimal_model_or_data,
        &d.first_discriminating_test,
        &d.likely_failure_mode,
        &d.next_action,
        &d.drop_reason,
    ] {
        optional(field, 8_000)?;
    }
    linked(store, ws, &d.theory_ids, "theory")?;
    match (&d.status[..], &d.task_id) {
        ("converted", None) => {
            return Err(WorkbenchError::invalid(
                "A converted direction links the task it became",
            ))
        }
        (_, Some(task)) => {
            record(store, ws, task, "task")?;
        }
        _ => {}
    }
    if d.status == "dropped" && d.drop_reason.trim().is_empty() {
        return Err(WorkbenchError::invalid(
            "Record why this direction was dropped",
        ));
    }
    put(store, ws, &new_or_id(object_id)?, "direction", expected, &d)
}

pub(super) fn convert_direction(
    store: &Store,
    ws: &str,
    object_id: &str,
    expected: i64,
) -> WorkbenchResult<Value> {
    let r = record(store, ws, object_id, "direction")?;
    let mut d: ResearchDirection = decode(&r)?;
    if r.revision != expected {
        return Err(WorkbenchError::conflict(
            "This project record changed; refresh before editing",
        ));
    }
    if d.status == "converted" || d.status == "dropped" {
        return Err(WorkbenchError::invalid(
            "Only idea or active directions convert into a task",
        ));
    }
    if d.first_discriminating_test.trim().is_empty() {
        return Err(WorkbenchError::invalid(
            "State the first discriminating test before converting this direction into a task",
        ));
    }
    let objective = format!(
        "{}\n\nFirst discriminating test: {}",
        d.question.trim(),
        d.first_discriminating_test.trim()
    );
    let task = tasks::create_task(
        store,
        ws,
        objective,
        None,
        Vec::new(),
        vec![d.first_discriminating_test.trim().to_string()],
    )?;
    d.status = "converted".into();
    d.task_id = Some(task.id.clone());
    let direction = put(store, ws, object_id, "direction", expected, &d)?;
    Ok(json!({"direction":direction,"task":task}))
}

fn comment(path: &str, text: &str) -> String {
    if path.ends_with(".md") {
        format!("<!-- {text} -->\n")
    } else {
        format!("% {text}\n")
    }
}
pub(super) fn promote_theory(
    store: &Store,
    ws: &str,
    cp: &str,
    path: &str,
    theory_id: &str,
    after_line: Option<usize>,
) -> WorkbenchResult<Value> {
    if !(path.ends_with(".tex") || path.ends_with(".md")) {
        return Err(WorkbenchError::invalid(
            "Promote derivation prose into a .tex or .md manuscript file",
        ));
    }
    let r = record(store, ws, theory_id, "theory")?;
    let note: TheoryNote = decode(&r)?;
    if matches!(note.kind.as_str(), "rejected_approach" | "unresolved_step")
        || note.status == "abandoned"
    {
        return Err(WorkbenchError::invalid(
            "Rejected approaches and unresolved steps are working notes; they are not promoted",
        ));
    }
    if note.body.trim().is_empty() {
        return Err(WorkbenchError::invalid(
            "This note has no derivation prose to promote",
        ));
    }
    let safe = files::SafeRoot::open(&session_task_root(store, ws, cp)?)?;
    let existing = safe.optional_read(path)?;
    let old_hash = existing.as_deref().map(hash);
    let current = match &existing {
        Some(bytes) => String::from_utf8(bytes.clone())
            .map_err(|_| WorkbenchError::invalid("The target file is not UTF-8 text"))?,
        None => String::new(),
    };
    let mut block = comment(
        path,
        &format!(
            "Pipeline theory note {theory_id} revision {} ({}): {}",
            r.revision,
            note.kind,
            note.title.replace(['\n', '\r'], " ")
        ),
    );
    let mut assumptions = note.assumptions.clone();
    for id in &note.assumption_ids {
        if let Ok(a) = record(store, ws, id, "theory") {
            if let Ok(a) = decode::<TheoryNote>(&a) {
                assumptions.push(format!("{} [{id}]", a.statement));
            }
        }
    }
    if !assumptions.is_empty() {
        block.push_str(&comment(
            path,
            &format!(
                "Assumptions referenced, unchanged by this promotion: {}",
                assumptions
                    .iter()
                    .map(|a| a.replace(['\n', '\r'], " "))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ));
    }
    if !note.unresolved_steps.is_empty() {
        block.push_str(&comment(
            path,
            &format!(
                "Unresolved steps retained: {}",
                note.unresolved_steps
                    .iter()
                    .map(|s| s.replace(['\n', '\r'], " "))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ));
    }
    let body = note.body.trim_end();
    if (path.ends_with(".md") && body.contains("-->")) || body.contains('\0') {
        return Err(WorkbenchError::invalid(
            "The note body contains text that would break the promotion markers",
        ));
    }
    block.push_str(body);
    block.push('\n');
    block.push_str(&comment(path, &format!("End theory note {theory_id}")));
    let content = match after_line {
        None => {
            let mut c = current.clone();
            if !c.is_empty() && !c.ends_with('\n') {
                c.push('\n');
            }
            c.push_str(&block);
            c
        }
        Some(n) => {
            let lines: Vec<&str> = current.split_inclusive('\n').collect();
            if n > lines.len() {
                return Err(WorkbenchError::invalid(format!(
                    "The file has {} lines; choose an insertion line within it",
                    lines.len()
                )));
            }
            let mut c = String::new();
            for line in &lines[..n] {
                c.push_str(line);
            }
            if !c.is_empty() && !c.ends_with('\n') {
                c.push('\n');
            }
            c.push_str(&block);
            for line in &lines[n..] {
                c.push_str(line);
            }
            c
        }
    };
    if content.len() > 2 * 1024 * 1024 {
        return Err(WorkbenchError::invalid(
            "Promotion targets support files through 2 MiB",
        ));
    }
    stage_text(store, ws, cp, path, old_hash.as_deref(), &content)?;
    let checkpoint = tasks::capture_changes(store, ws, cp)?;
    let mut updated = note;
    updated.promotions.push(Promotion {
        checkpoint_id: cp.into(),
        path: path.into(),
        note_revision: r.revision,
        promoted_at: now(),
    });
    let note = put(store, ws, theory_id, "theory", r.revision, &updated)?;
    Ok(json!({"checkpoint":checkpoint,"note":note}))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TheoryEvidence {
    pub analytical: usize,
    pub symbolic: usize,
    pub numerical_passed: usize,
    pub numerical_failed: usize,
    pub counterexamples: usize,
    pub heuristic: usize,
    pub model_assessment: usize,
    pub label: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TheoryOverview {
    pub notes: Vec<ProjectRecord>,
    pub checks: Vec<ProjectRecord>,
    pub directions: Vec<ProjectRecord>,
    pub evidence: BTreeMap<String, TheoryEvidence>,
}
pub fn theory_overview(store: &Store, ws: &str) -> WorkbenchResult<TheoryOverview> {
    let notes = records(store, ws, "theory")?;
    let checks = records(store, ws, "check")?;
    let directions = records(store, ws, "direction")?;
    let mut evidence = BTreeMap::new();
    for note in &notes {
        let n: TheoryNote = decode(note)?;
        let mut e = TheoryEvidence {
            analytical: 0,
            symbolic: 0,
            numerical_passed: 0,
            numerical_failed: 0,
            counterexamples: 0,
            heuristic: 0,
            model_assessment: 0,
            label: String::new(),
        };
        for c in &checks {
            let c: TheoryCheckRecord = decode(c)?;
            if c.check.theory_id != note.id {
                continue;
            }
            let passed = c.check.outcome == "passed";
            match c.check.method.as_str() {
                "analytical_argument" if passed => e.analytical += 1,
                "symbolic_identity" if passed => e.symbolic += 1,
                "numerical_verification" if passed => e.numerical_passed += 1,
                "numerical_verification" if c.check.outcome == "failed" => e.numerical_failed += 1,
                "numerical_counterexample" if passed => e.counterexamples += 1,
                "heuristic" => e.heuristic += 1,
                "model_assessment" => e.model_assessment += 1,
                _ => {}
            }
        }
        e.label = if n.status == "abandoned" {
            "Abandoned approach; retained with its assumptions and reason.".into()
        } else if !n.unresolved_steps.is_empty() {
            format!(
                "{} unresolved step(s) remain; this is not a complete proof.",
                n.unresolved_steps.len()
            )
        } else if e.counterexamples > 0 {
            "A confirmed counterexample is recorded against the general statement.".into()
        } else if e.numerical_failed > 0 && e.analytical == 0 {
            "A numerical check failed; no analytical argument is recorded.".into()
        } else if e.analytical > 0 {
            "Analytical argument recorded by the researcher; not machine-verified.".into()
        } else if e.symbolic > 0 {
            "Symbolic identity checked on its stated domain.".into()
        } else if e.numerical_passed > 0 {
            "Numerical checks pass on tested instances only; no general argument is recorded."
                .into()
        } else if e.heuristic + e.model_assessment > 0 {
            "Only heuristic or model assessment recorded; nothing is verified.".into()
        } else {
            "No checks recorded.".into()
        };
        evidence.insert(note.id.clone(), e);
    }
    Ok(TheoryOverview {
        notes,
        checks,
        directions,
        evidence,
    })
}

/// Bounded text for the automatic project context: abandoned approaches only,
/// so a later session finds them without treating working notes as decisions.
pub fn rejected_approach_context(store: &Store, ws: &str) -> WorkbenchResult<String> {
    let notes = records(store, ws, "theory")?;
    let mut text = String::new();
    let mut shown = 0;
    for record in &notes {
        let n: TheoryNote = decode(record)?;
        if n.status != "abandoned" {
            continue;
        }
        if shown == 20 {
            text.push_str(
                "\nFurther abandoned approaches are omitted; read theory records for the rest.\n",
            );
            break;
        }
        shown += 1;
        let mut reason = n.rejection_reason.trim().to_string();
        if reason.len() > 500 {
            let mut end = 500;
            while !reason.is_char_boundary(end) {
                end -= 1;
            }
            reason.truncate(end);
            reason.push('…');
        }
        text.push_str(&format!(
            "\nAbandoned approach {} ({}): {}\n  Statement: {}\n  Reason: {}\n",
            record.id,
            n.kind,
            n.title.trim(),
            n.statement.trim().chars().take(300).collect::<String>(),
            reason
        ));
        if !n.assumptions.is_empty() || !n.assumption_ids.is_empty() {
            text.push_str(&format!(
                "  Assumptions: {}\n",
                n.assumptions
                    .iter()
                    .map(|a| a.chars().take(200).collect::<String>())
                    .chain(n.assumption_ids.iter().map(|id| format!("[{id}]")))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }
    if !notes.is_empty() {
        text.push_str(&format!(
            "\n{} theory note(s) are retained as working records; read them with the research records tool. Their status is the researcher's; numerical checks never establish general statements.\n",
            notes.len()
        ));
    }
    Ok(text)
}
