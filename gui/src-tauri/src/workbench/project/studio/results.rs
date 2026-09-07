use super::*;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Experiment {
    pub question: String,
    pub baseline_execution_id: String,
    pub intended_change: String,
    pub execution_ids: Vec<String>,
    pub interpretation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecificationField {
    pub value: Value,
    pub origin: String,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Specification {
    pub specification_id: String,
    pub sample_id: String,
    pub fields: BTreeMap<String, SpecificationField>,
}
pub(super) fn save_experiment(
    store: &Store,
    ws: &str,
    object_id: Option<&str>,
    expected: i64,
    e: Experiment,
) -> WorkbenchResult<ProjectRecord> {
    bounded(&e.question, 4000)?;
    bounded(&e.intended_change, 10000)?;
    if e.execution_ids.len() > 100 || e.interpretation.len() > 64 * 1024 {
        return Err(WorkbenchError::invalid("Experiment exceeds its limits"));
    }
    if execution(store, ws, &e.baseline_execution_id)?.outcome != "completed" {
        return Err(WorkbenchError::invalid(
            "Select a completed baseline execution",
        ));
    }
    for id in &e.execution_ids {
        execution(store, ws, id)?;
    }
    put(
        store,
        ws,
        &new_or_id(object_id)?,
        "experiment",
        expected,
        &e,
    )
}
pub(super) fn save_specification(
    store: &Store,
    ws: &str,
    object_id: Option<&str>,
    expected: i64,
    s: Specification,
) -> WorkbenchResult<ProjectRecord> {
    valid_id(&s.specification_id)?;
    valid_id(&s.sample_id)?;
    if s.fields.len() > 100 || serde_json::to_vec(&s).map_err(err)?.len() > 128 * 1024 {
        return Err(WorkbenchError::invalid(
            "Specification metadata exceeds its limit",
        ));
    }
    for (name, f) in &s.fields {
        bounded(name, 200)?;
        if !matches!(f.origin.as_str(), "declared" | "inferred") {
            return Err(WorkbenchError::invalid(
                "Specification fields must distinguish declared from inferred values",
            ));
        }
    }
    let stable = format!(
        "spec_{}",
        hash(format!("{ws}\0{}\0{}", s.specification_id, s.sample_id).as_bytes())
    );
    if object_id.is_some_and(|id| id != stable) {
        return Err(WorkbenchError::invalid(
            "Specification identity cannot be renamed",
        ));
    }
    put(store, ws, &stable, "specification", expected, &s)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResultRef {
    pub execution_id: String,
    pub result_id: String,
}
pub(super) fn result(
    store: &Store,
    ws: &str,
    r: &ResultRef,
) -> WorkbenchResult<research::ResearchResultV1> {
    execution(store, ws, &r.execution_id)?;
    let encoded: String = store
        .connection()?
        .query_row(
            "SELECT body_json FROM structured_results WHERE execution_id=?1 AND result_id=?2",
            params![r.execution_id, r.result_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(err)?
        .ok_or_else(|| WorkbenchError::invalid("Structured result is unavailable"))?;
    serde_json::from_str(&encoded).map_err(err)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImpulseResponse {
    pub result_id: String,
    pub variable: String,
    pub units: String,
    pub shock_normalization: String,
    pub horizon_unit: String,
    pub horizons: Vec<f64>,
    pub values: Vec<Option<f64>>,
    pub specification_id: String,
    pub sample_id: String,
}
pub(super) fn validate_series(s: &ImpulseResponse) -> WorkbenchResult<()> {
    valid_id(&s.result_id)?;
    for text in [
        &s.variable,
        &s.units,
        &s.shock_normalization,
        &s.horizon_unit,
        &s.specification_id,
        &s.sample_id,
    ] {
        bounded(text, 1000)?;
    }
    if s.horizons.is_empty()
        || s.horizons.len() > 1000
        || s.horizons.len() != s.values.len()
        || s.horizons.iter().any(|v| !v.is_finite())
        || s.horizons.windows(2).any(|p| p[0] >= p[1])
        || s.values.iter().flatten().any(|v| !v.is_finite())
    {
        return Err(WorkbenchError::invalid("Impulse responses require finite, increasing horizons and equally sized finite or missing values"));
    }
    Ok(())
}
pub(super) fn import_results(
    store: &Store,
    ws: &str,
    execution_id: &str,
    artifact_id: &str,
) -> WorkbenchResult<Value> {
    let e = execution(store, ws, execution_id)?;
    if e.outcome != "completed" {
        return Err(WorkbenchError::invalid(
            "Only successful adopted outputs can supply results",
        ));
    }
    let (bytes, a) = artifact_bytes(store, ws, &e, artifact_id)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(WorkbenchError::invalid("Results export exceeds 4 MiB"));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(err)?;
    if !matches!(
        value["schema"].as_str(),
        Some("research-results-v1" | "research-results-v2")
    ) {
        return Err(WorkbenchError::invalid(
            "Expected research-results-v1 or research-results-v2",
        ));
    }
    let raw = value["results"].as_array().cloned().unwrap_or_default();
    let raw_series = value["series"].as_array().cloned().unwrap_or_default();
    if raw.len() + raw_series.len() > 1000 || raw.len() + raw_series.len() == 0 {
        return Err(WorkbenchError::invalid(
            "Export must contain 1–1000 results",
        ));
    }
    if !raw_series.is_empty() && value["schema"] != "research-results-v2" {
        return Err(WorkbenchError::invalid("Series require the v2 contract"));
    }
    // Validate the complete package before recording any result. The host overwrites provenance.
    let mut prepared = Vec::new();
    let mut ids = BTreeSet::new();
    for (index, mut v) in raw.into_iter().enumerate() {
        v["sourceExecutionId"] = json!(execution_id);
        v["artifactLocator"] = json!(format!(
            "{}#results/{index}",
            a["path"].as_str().unwrap_or(artifact_id)
        ));
        let r: research::ResearchResultV1 = serde_json::from_value(v).map_err(err)?;
        research::validate_result(&r)?;
        if !r.estimate.is_finite()
            || r.standard_error.is_some_and(|v| !v.is_finite() || v < 0.)
            || r.confidence_interval
                .is_some_and(|v| v.iter().any(|v| !v.is_finite()) || v[0] > v[1])
            || !ids.insert(r.result_id.clone())
        {
            return Err(WorkbenchError::invalid(
                "Results require unique IDs, finite values and valid uncertainty intervals",
            ));
        }
        if let Ok(old) = result(
            store,
            ws,
            &ResultRef {
                execution_id: e.id.clone(),
                result_id: r.result_id.clone(),
            },
        ) {
            if old != r {
                return Err(WorkbenchError::conflict(
                    "This execution already has a different immutable result with this ID",
                ));
            }
        }
        prepared.push(r);
    }
    let series = raw_series
        .into_iter()
        .map(serde_json::from_value::<ImpulseResponse>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    for s in &series {
        validate_series(s)?;
        if !ids.insert(s.result_id.clone()) {
            return Err(WorkbenchError::invalid("Duplicate result or series ID"));
        }
    }
    for (index, s) in series.iter().enumerate() {
        let rid = format!(
            "series_{}",
            hash(format!("{}\0{}", e.id, s.result_id).as_bytes())
        );
        let body = json!({"series":s,"executionId":e.id,"artifactId":artifact_id,"artifactHash":a["contentHash"],"locator":format!("series/{index}"),"provenance":"host_adopted_output"});
        if let Ok(old) = record(store, ws, &rid, "series") {
            if old.body != body {
                return Err(WorkbenchError::conflict(
                    "An immutable series with this identity already differs",
                ));
            }
        }
    }
    for r in &prepared {
        if result(
            store,
            ws,
            &ResultRef {
                execution_id: e.id.clone(),
                result_id: r.result_id.clone(),
            },
        )
        .is_err()
        {
            research::record_structured_result(
                store,
                research::RecordStructuredResultRequest {
                    workspace_id: ws.into(),
                    result: r.clone(),
                    operation_id: id("import_result")?,
                },
            )?;
        }
    }
    for (index, s) in series.iter().enumerate() {
        let rid = format!(
            "series_{}",
            hash(format!("{}\0{}", e.id, s.result_id).as_bytes())
        );
        let body = json!({"series":s,"executionId":e.id,"artifactId":artifact_id,"artifactHash":a["contentHash"],"locator":format!("series/{index}"),"provenance":"host_adopted_output"});
        if let Ok(old) = record(store, ws, &rid, "series") {
            if old.body != body {
                return Err(WorkbenchError::conflict(
                    "An immutable series with this identity already differs",
                ));
            }
        } else {
            put(store, ws, &rid, "series", 0, &body)?;
        }
    }
    Ok(
        json!({"results":prepared,"series":series,"executionId":e.id,"artifactId":artifact_id,"artifactHash":a["contentHash"],"provenance":"host_adopted_output"}),
    )
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitConversion {
    pub from_units: String,
    pub to_units: String,
    pub factor: f64,
    pub offset: f64,
    pub rationale: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareExperimentRequest {
    pub workspace_id: String,
    pub left: ResultRef,
    pub right: ResultRef,
    pub rationale: String,
    pub conversion: Option<UnitConversion>,
    pub relative_meaningful: bool,
}
pub fn compare_experiment(store: &Store, r: CompareExperimentRequest) -> WorkbenchResult<Value> {
    let left = result(store, &r.workspace_id, &r.left)?;
    let mut right = result(store, &r.workspace_id, &r.right)?;
    let mut blockers = Vec::new();
    let mut differences = Vec::new();
    if let Some(c) = &r.conversion {
        if c.from_units != right.units
            || c.to_units != left.units
            || !c.factor.is_finite()
            || c.factor <= 0.
            || !c.offset.is_finite()
            || c.rationale.trim().is_empty()
        {
            return Err(WorkbenchError::invalid("Record an explicit positive affine conversion from alternative units to baseline units"));
        }
        right.estimate = right.estimate * c.factor + c.offset;
        right.standard_error = right.standard_error.map(|v| v * c.factor);
        right.confidence_interval = right
            .confidence_interval
            .map(|v| [v[0] * c.factor + c.offset, v[1] * c.factor + c.offset]);
        right.units = c.to_units.clone();
    }
    if left.units != right.units {
        blockers.push("units");
    }
    if left.estimand != right.estimand {
        blockers.push("estimand");
    }
    if left.transformation != right.transformation {
        blockers.push("transformation");
    }
    for (name, l, rr) in [
        (
            "specification",
            &left.specification_id,
            &right.specification_id,
        ),
        ("sample", &left.sample_id, &right.sample_id),
    ] {
        if l != rr {
            differences.push(name);
        }
    }
    if !differences.is_empty() && r.rationale.trim().is_empty() {
        blockers.push("A comparison across specifications/samples needs a rationale");
    }
    let change = right.estimate - left.estimate;
    if !change.is_finite() {
        blockers.push("nonfinite change");
    }
    let comparable = blockers.is_empty();
    let metadata = records(store, &r.workspace_id, "specification")?;
    let metadata_for = |v: &research::ResearchResultV1| {
        metadata.iter().find(|s| {
            s.body["specificationId"] == v.specification_id && s.body["sampleId"] == v.sample_id
        })
    };
    Ok(
        json!({"comparable":comparable,"blockers":blockers,"specificationDifferences":differences,"left":left,"rightConverted":right,"leftSpecification":metadata_for(&left),"rightSpecification":metadata_for(&right),"signedChange":if comparable{Some(change)}else{None},"absoluteChange":if comparable{Some(change.abs())}else{None},"relativeChange":if comparable&&r.relative_meaningful&&left.estimate!=0.&&(change/left.estimate.abs()).is_finite(){Some(change/left.estimate.abs())}else{None},"nChange":left.n.zip(right.n).map(|(l,r)|r as i128-l as i128),"conversion":r.conversion,"rationale":r.rationale,"checks":{"numericalComparison":comparable,"solverConvergence":"not_assessed","economicInterpretation":"not_assessed"}}),
    )
}
pub fn compare_series(
    store: &Store,
    ws: &str,
    left_id: &str,
    right_id: &str,
    rationale: &str,
) -> WorkbenchResult<Value> {
    let l = record(store, ws, left_id, "series")?;
    let r = record(store, ws, right_id, "series")?;
    let a: ImpulseResponse = serde_json::from_value(l.body["series"].clone()).map_err(err)?;
    let b: ImpulseResponse = serde_json::from_value(r.body["series"].clone()).map_err(err)?;
    let mut blockers = Vec::new();
    for (label, s, t) in [
        ("variable", &a.variable, &b.variable),
        ("units", &a.units, &b.units),
        (
            "shock normalization",
            &a.shock_normalization,
            &b.shock_normalization,
        ),
        ("horizon unit", &a.horizon_unit, &b.horizon_unit),
    ] {
        if s != t {
            blockers.push(label);
        }
    }
    if a.horizons != b.horizons {
        blockers.push("horizon axes differ; no implicit interpolation");
    }
    if (a.specification_id != b.specification_id || a.sample_id != b.sample_id)
        && rationale.trim().is_empty()
    {
        blockers.push("Specification/sample comparison needs a rationale");
    }
    let changes = if blockers.is_empty() {
        a.values
            .iter()
            .zip(&b.values)
            .map(|(a, b)| {
                a.zip(*b).and_then(|(a, b)| {
                    let d = b - a;
                    d.is_finite().then_some(d)
                })
            })
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    Ok(
        json!({"comparable":blockers.is_empty(),"blockers":blockers,"horizons":a.horizons,"changes":changes,"left":l,"right":r,"rationale":rationale,"missingValues":"unavailable, never zero"}),
    )
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumericBinding {
    pub result: ResultRef,
    pub anchor_id: String,
    pub role: String,
    pub component: String,
    pub printed: String,
    pub precision: u8,
    pub reported_units: String,
    pub origin: String,
    pub confirmed: bool,
}
fn component(r: &research::ResearchResultV1, key: &str) -> WorkbenchResult<Option<f64>> {
    match key {
        "estimate" => Ok(Some(r.estimate)),
        "standardError" => Ok(r.standard_error),
        "intervalLower" => Ok(r.confidence_interval.map(|v| v[0])),
        "intervalUpper" => Ok(r.confidence_interval.map(|v| v[1])),
        _ => Err(WorkbenchError::invalid(
            "Choose estimate, standard error or an interval endpoint",
        )),
    }
}
pub(super) fn bind_number(
    store: &Store,
    ws: &str,
    object_id: Option<&str>,
    expected: i64,
    b: NumericBinding,
) -> WorkbenchResult<ProjectRecord> {
    let r = result(store, ws, &b.result)?;
    component(&r, &b.component)?;
    let a: Annotation = decode(&record(store, ws, &b.anchor_id, "anchor")?)?;
    if b.precision > 12
        || !matches!(b.role.as_str(), "prose" | "table" | "figure")
        || !matches!(b.origin.as_str(), "manual" | "proposed")
    {
        return Err(WorkbenchError::invalid("Invalid numeric binding"));
    }
    bounded(&b.printed, 100)?;
    bounded(&b.reported_units, 300)?;
    if a.selection.quote.matches(&b.printed).count() != 1 && b.confirmed {
        return Err(WorkbenchError::invalid(
            "A confirmed binding needs one exact, unambiguous printed value in its passage",
        ));
    }
    put(store, ws, &new_or_id(object_id)?, "binding", expected, &b)
}
fn result_freshness(
    store: &Store,
    ws: &str,
    r: &ResultRef,
    started: Instant,
    read: &mut usize,
) -> WorkbenchResult<Value> {
    let e = execution(store, ws, &r.execution_id)?;
    let registered = root(store, ws)?;
    let cwd = Path::new(&e.cwd);
    if !cwd.starts_with(&registered) || !cwd.is_dir() {
        return Ok(json!({"state":"unknown","reason":"Execution root is no longer available"}));
    }
    let safe = files::SafeRoot::open(cwd)?;
    let mut changed = Vec::new();
    let mut unknown = Vec::new();
    for f in e.input_manifest["files"].as_array().into_iter().flatten() {
        let path = f["path"].as_str().unwrap_or("");
        if started.elapsed() > Duration::from_secs(2) || *read > 32 * 1024 * 1024 {
            unknown.push(path.to_string());
            continue;
        }
        match safe.read(path) {
            Ok(bytes) => {
                *read += bytes.len();
                if f["contentHash"].as_str() != Some(&hash(&bytes)) {
                    changed.push(path.to_string());
                }
            }
            Err(_) => unknown.push(path.to_string()),
        }
    }
    let state = if !changed.is_empty() {
        "stale"
    } else if !unknown.is_empty()
        || e.input_manifest["files"]
            .as_array()
            .is_none_or(Vec::is_empty)
    {
        "unknown"
    } else {
        "current"
    };
    Ok(
        json!({"state":state,"changedInputs":changed,"unavailableInputs":unknown,"coverage":"declared inputs only","executionId":e.id,"dependencyHash":e.dependency_hash}),
    )
}
pub fn binding_coverage(store: &Store, ws: &str) -> WorkbenchResult<Vec<Value>> {
    let started = Instant::now();
    let mut read = 0usize;
    let current = selected_manuscript(store, ws)?;
    let mut output = Vec::new();
    let mut cache: BTreeMap<String, Value> = BTreeMap::new();
    for entry in records(store, ws, "binding")? {
        let b: NumericBinding = decode(&entry)?;
        let r = result(store, ws, &b.result);
        let a: Annotation = decode(&record(store, ws, &b.anchor_id, "anchor")?)?;
        let mut reasons: Vec<String> = Vec::new();
        let mut state = "current";
        if !b.confirmed {
            state = "unknown";
            reasons.push("Proposed link requires explicit confirmation".into());
        }
        if current
            .as_ref()
            .is_some_and(|id| id != &a.selection.revision_id)
        {
            state = "stale";
            reasons.push("The accepted manuscript revision differs from this exact passage".into());
        }
        let dependency = if let Some(v) = cache.get(&b.result.execution_id) {
            v.clone()
        } else {
            let v = result_freshness(store, ws, &b.result, started, &mut read)
                .unwrap_or_else(|e| json!({"state":"unknown","reason":e.message}));
            cache.insert(b.result.execution_id.clone(), v.clone());
            v
        };
        if dependency["state"] == "stale" {
            state = "stale";
            reasons.push("Declared execution inputs changed".into());
        } else if dependency["state"] == "unknown" && state == "current" {
            state = "unknown";
        }
        let expected = match &r {
            Ok(r) => {
                if b.reported_units != r.units {
                    state = "stale";
                    reasons.push("Reported units differ from result units".into());
                }
                component(r, &b.component)?
            }
            Err(_) => {
                state = "unavailable";
                None
            }
        };
        let printed = b
            .printed
            .replace('−', "-")
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite());
        let numeric_passed = expected.zip(printed).map(|(actual, printed)| {
            let scale = 10_f64.powi(b.precision as i32);
            (actual * scale).is_finite()
                && ((actual * scale).round() / scale - printed).abs() < 0.5 / scale
                && !(actual.signum() != printed.signum() && printed != 0.)
        });
        if numeric_passed == Some(false) {
            state = "stale";
            reasons.push("Printed value or sign disagrees at the declared precision".into());
        }
        if expected.is_none() {
            state = "unavailable";
        }
        if numeric_passed.is_none() && state == "current" {
            state = "unknown";
        }
        output.push(json!({"record":entry,"state":state,"reasons":reasons,"numericPassed":numeric_passed,"expected":expected,"dependency":dependency,"result":r.ok(),"anchor":a}));
    }
    Ok(output)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValueMacro {
    pub name: String,
    pub result: ResultRef,
    pub precision: u8,
}
pub(super) fn generate_values(
    store: &Store,
    ws: &str,
    cp: &str,
    path: &str,
    values: Vec<ValueMacro>,
) -> WorkbenchResult<Value> {
    if !path.ends_with(".tex") || values.is_empty() || values.len() > 100 {
        return Err(WorkbenchError::invalid(
            "Choose a TeX value macro file with 1–100 quantities",
        ));
    }
    let mut content = String::from(
        "% Generated from immutable Pipeline results. Review this change before acceptance.\n",
    );
    let mut names = BTreeSet::new();
    for v in values {
        if v.name.is_empty()
            || v.name.len() > 80
            || !v.name.bytes().all(|c| c.is_ascii_alphabetic())
            || !names.insert(v.name.clone())
            || v.precision > 12
        {
            return Err(WorkbenchError::invalid(
                "Macro names must be unique ASCII letters; precision is 0–12",
            ));
        }
        let r = result(store, ws, &v.result)?;
        content.push_str(&format!(
            "% {} / {} [{}]\n\\newcommand{{\\{}}}{{{:.*}}}\n",
            r.source_execution_id,
            r.result_id,
            r.units.replace(['\n', '\r'], " "),
            v.name,
            v.precision as usize,
            r.estimate
        ));
    }
    let safe = files::SafeRoot::open(&session_task_root(store, ws, cp)?)?;
    let old = safe.optional_read(path)?.map(|b| hash(&b));
    stage_text(store, ws, cp, path, old.as_deref(), &content)?;
    Ok(json!(tasks::capture_changes(store, ws, cp)?))
}
