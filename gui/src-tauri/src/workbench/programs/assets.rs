use super::*;
use crate::workbench::{project, research};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualValue {
    pub value: f64,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub label: String,
    pub source: ResearchObjectRef,
    pub manual: Option<ManualValue>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetSpec {
    pub kind: String,
    pub title: String,
    pub entries: Vec<Entry>,
    pub digits: usize,
    pub uncertainty: String,
    pub notes: String,
    pub sample_comparison_rationale: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationAsset {
    pub style_version: u32,
    pub spec: AssetSpec,
    pub entries: Vec<Value>,
    pub artifacts: Vec<Artifact>,
    pub sources: Vec<ResearchObjectRef>,
    pub execution_id: Option<String>,
}
pub fn resolve(store: &Store, ws: &str, spec: &AssetSpec) -> WorkbenchResult<Vec<Value>> {
    bounded(&spec.title, 300)?;
    if spec.entries.is_empty()
        || spec.entries.len() > 40
        || spec.digits > 10
        || spec.notes.len() > 4000
        || !["table", "coefficient", "irf"].contains(&spec.kind.as_str())
        || !["none", "se", "ci"].contains(&spec.uncertainty.as_str())
    {
        return Err(WorkbenchError::invalid(
            "Invalid asset format, precision or number of entries",
        ));
    }
    if spec.kind != "table" && spec.entries.len() > 12 {
        return Err(WorkbenchError::invalid(
            "Use at most 12 series or coefficients in a publication figure",
        ));
    }
    let mut values: Vec<Value> = Vec::new();
    for entry in &spec.entries {
        bounded(&entry.label, 300)?;
        let original = super::super::search::read_object(store, ws, &entry.source, 64 * 1024)?;
        if original.truncated {
            return Err(WorkbenchError::invalid(
                "Select the complete numeric result or series, not an excerpt",
            ));
        }
        let mut value: Value = serde_json::from_str(&original.text).map_err(err)?;
        if spec.kind == "irf" {
            if entry.source.kind != "record"
                || value["series"].is_null()
                || value["provenance"] != "host_adopted_output"
            {
                return Err(WorkbenchError::invalid(
                    "IRF assets require an adopted numeric series",
                ));
            }
            project::record(store, ws, &entry.source.id, "series")?;
            let execution = research::get_execution(
                store,
                value["executionId"]
                    .as_str()
                    .ok_or_else(|| WorkbenchError::invalid("Series lacks its source execution"))?,
            )?;
            let artifact_id = value["artifactId"]
                .as_str()
                .ok_or_else(|| WorkbenchError::invalid("Series lacks its adopted artifact"))?;
            if execution.workspace_id != ws
                || execution.outcome != "completed"
                || !execution.output_manifest["artifacts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|a| {
                        a["artifactId"] == artifact_id && a["contentHash"] == value["artifactHash"]
                    })
            {
                return Err(WorkbenchError::invalid(
                    "Series provenance is unavailable or differs from the adopted execution output",
                ));
            }
            let bytes = artifact_bytes(store, ws, artifact_id, 8 * 1024 * 1024)?;
            let original: Value = serde_json::from_slice(&bytes).map_err(err)?;
            let pointer = format!("/{}", value["locator"].as_str().unwrap_or(""));
            if desk::hash(&bytes) != value["artifactHash"].as_str().unwrap_or("")
                || original.pointer(&pointer) != Some(&value["series"])
            {
                return Err(WorkbenchError::invalid(
                    "Series values differ from their retained output artifact",
                ));
            }
        } else {
            let result: research::ResearchResultV1 =
                serde_json::from_value(value.clone()).map_err(err)?;
            research::validate_result(&result)?;
            if entry.source.kind != "result" {
                return Err(WorkbenchError::invalid(
                    "Choose an immutable structured result",
                ));
            }
        }
        if spec.kind == "irf" {
            let series: project::ImpulseResponse =
                serde_json::from_value(value["series"].clone()).map_err(err)?;
            if series.horizons.len() != series.values.len()
                || series.horizons.is_empty()
                || series.horizons.len() > 2000
                || series.horizons.iter().any(|v| !v.is_finite())
                || series.values.iter().flatten().any(|v| !v.is_finite())
                || series.horizons.windows(2).any(|v| v[0] >= v[1])
            {
                return Err(WorkbenchError::invalid(
                    "IRF horizons and values are invalid",
                ));
            }
        }
        if let Some(manual) = &entry.manual {
            if spec.kind == "irf" || !manual.value.is_finite() {
                return Err(WorkbenchError::invalid(
                    "Manual overrides require a finite scalar",
                ));
            }
            bounded(&manual.reason, 4000)?;
            value["originalEstimate"] = value["estimate"].clone();
            value["estimate"] = json!(manual.value);
            value["standardError"] = Value::Null;
            value["confidenceInterval"] = Value::Null;
            value["manualReason"] = json!(manual.reason);
        }
        if let Some(first) = values.first() {
            let f = &first["value"];
            let (f, v) = if spec.kind == "irf" {
                (&f["series"], &value["series"])
            } else {
                (f, &value)
            };
            let fields = if spec.kind == "irf" {
                vec!["units", "variable", "shockNormalization", "horizonUnit"]
            } else {
                vec!["units", "estimand", "transformation"]
            };
            for field in fields {
                if f[field] != v[field] {
                    return Err(WorkbenchError::invalid(format!(
                        "Asset entries have incompatible {field}"
                    )));
                }
            }
            if f["sampleId"] != v["sampleId"]
                && spec
                    .sample_comparison_rationale
                    .as_ref()
                    .is_none_or(|s| s.trim().is_empty())
            {
                return Err(WorkbenchError::invalid(
                    "Different samples need an explicit comparison rationale",
                ));
            }
        }
        values.push(json!({"label":entry.label,"source":entry.source,"value":value,"manual":entry.manual.is_some()}));
    }
    Ok(values)
}
pub(super) fn tex(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'α' => "\\ensuremath{\\alpha}".into(),
            'β' => "\\ensuremath{\\beta}".into(),
            'γ' => "\\ensuremath{\\gamma}".into(),
            'δ' => "\\ensuremath{\\delta}".into(),
            'ε' => "\\ensuremath{\\epsilon}".into(),
            'θ' => "\\ensuremath{\\theta}".into(),
            'λ' => "\\ensuremath{\\lambda}".into(),
            'μ' => "\\ensuremath{\\mu}".into(),
            'π' => "\\ensuremath{\\pi}".into(),
            'ρ' => "\\ensuremath{\\rho}".into(),
            'σ' => "\\ensuremath{\\sigma}".into(),
            'τ' => "\\ensuremath{\\tau}".into(),
            'φ' => "\\ensuremath{\\phi}".into(),
            'ω' => "\\ensuremath{\\omega}".into(),
            'Δ' => "\\ensuremath{\\Delta}".into(),
            'Σ' => "\\ensuremath{\\Sigma}".into(),
            '±' => "\\ensuremath{\\pm}".into(),
            '≤' => "\\ensuremath{\\leq}".into(),
            '≥' => "\\ensuremath{\\geq}".into(),
            '−' => "-".into(),
            '\\' => "\\textbackslash{}".into(),
            '&' => "\\&".into(),
            '%' => "\\%".into(),
            '$' => "\\$".into(),
            '#' => "\\#".into(),
            '_' => "\\_".into(),
            '{' => "\\{".into(),
            '}' => "\\}".into(),
            '~' => "\\textasciitilde{}".into(),
            '^' => "\\textasciicircum{}".into(),
            _ => c.to_string(),
        })
        .collect()
}
fn csv(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
fn number(v: &Value, digits: usize) -> String {
    v.as_f64()
        .map(|n| format!("{n:.digits$}"))
        .unwrap_or_else(|| "Missing".into())
}
pub fn table(
    store: &Store,
    ws: &str,
    spec: AssetSpec,
    supersedes: Option<&str>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    if spec.kind != "table" {
        return Err(WorkbenchError::invalid("Choose a table specification"));
    }
    let entries = resolve(store, ws, &spec)?;
    let mut md = format!(
        "# {}\n\n| Quantity | Estimate | Uncertainty | Units |\n|---|---:|---|---|\n",
        spec.title
    );
    let mut latex=format!("% Pipeline table style v1; source identities in the companion JSON.\n\\begin{{table}}[htbp]\n\\centering\\small\n\\caption{{{}}}\n\\begin{{tabular}}{{p{{0.28\\linewidth}}rp{{0.25\\linewidth}}p{{0.16\\linewidth}}}}\n\\hline\nQuantity & Estimate & Uncertainty & Units \\\\\n\\hline\n",tex(&spec.title));
    let mut delimited="label,estimate,standard_error,ci_lower,ci_upper,units,sample_id,specification_id,manual_reason,source_revision\n".to_string();
    for e in &entries {
        let v = &e["value"];
        let estimate = number(&v["estimate"], spec.digits);
        let uncertainty = match spec.uncertainty.as_str() {
            "se" => format!("SE {}", number(&v["standardError"], spec.digits)),
            "ci" => {
                if v["confidenceInterval"].is_array() {
                    format!(
                        "[{}, {}]",
                        number(&v["confidenceInterval"][0], spec.digits),
                        number(&v["confidenceInterval"][1], spec.digits)
                    )
                } else {
                    "Missing".into()
                }
            }
            _ => String::new(),
        };
        let label = format!(
            "{}{}",
            e["label"].as_str().unwrap_or(""),
            if e["manual"] == true { " (manual)" } else { "" }
        );
        let units = v["units"].as_str().unwrap_or("");
        md.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            label.replace('|', "\\|"),
            estimate,
            uncertainty,
            units.replace('|', "\\|")
        ));
        latex.push_str(&format!(
            "{} & {} & {} & {} \\\\\n",
            tex(&label),
            tex(&estimate),
            tex(&uncertainty),
            tex(units)
        ));
        let values = [
            label,
            v["estimate"].to_string(),
            v["standardError"]
                .as_f64()
                .map(|n| n.to_string())
                .unwrap_or_default(),
            v["confidenceInterval"][0]
                .as_f64()
                .map(|n| n.to_string())
                .unwrap_or_default(),
            v["confidenceInterval"][1]
                .as_f64()
                .map(|n| n.to_string())
                .unwrap_or_default(),
            units.into(),
            v["sampleId"].as_str().unwrap_or("").into(),
            v["specificationId"].as_str().unwrap_or("").into(),
            v["manualReason"].as_str().unwrap_or("").into(),
            e["source"]["revision"].as_str().unwrap_or("").into(),
        ];
        delimited.push_str(&values.iter().map(|s| csv(s)).collect::<Vec<_>>().join(","));
        delimited.push('\n');
    }
    md.push_str(&format!("\n{}\n", spec.notes));
    latex.push_str(&format!("\\hline\n\\end{{tabular}}\n\\par\\smallskip\\begin{{minipage}}{{0.96\\linewidth}}\\footnotesize {}\\end{{minipage}}\n\\end{{table}}\n",tex(&spec.notes)));
    let artifacts = vec![
        blob(store, ws, md.as_bytes(), "md")?,
        blob(store, ws, latex.as_bytes(), "tex")?,
        blob(store, ws, delimited.as_bytes(), "csv")?,
        blob(
            store,
            ws,
            &serde_json::to_vec_pretty(&entries).map_err(err)?,
            "json",
        )?,
    ];
    let title = spec.title.clone();
    let asset = PublicationAsset {
        style_version: 1,
        sources: spec.entries.iter().map(|e| e.source.clone()).collect(),
        spec,
        entries,
        artifacts,
        execution_id: None,
    };
    desk::insert(
        store,
        ws,
        "publication_asset",
        &title,
        serde_json::to_value(asset).map_err(err)?,
        supersedes,
        operation,
    )
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FigureRequest {
    pub workspace_id: String,
    pub spec: AssetSpec,
    pub python_profile_id: String,
    pub operation_id: String,
}
pub fn figure_plan(store: &Store, r: FigureRequest) -> WorkbenchResult<DeskRecord> {
    if !["coefficient", "irf"].contains(&r.spec.kind.as_str()) {
        return Err(WorkbenchError::invalid("Choose a coefficient or IRF plot"));
    }
    bounded(&r.operation_id, 150)?;
    let request_hash = desk::hash(
        json!({"spec":r.spec,"profileId":r.python_profile_id})
            .to_string()
            .as_bytes(),
    );
    if let Some(prior) = desk::operation_record(store, &r.workspace_id, &r.operation_id)? {
        if prior.kind != "figure_recipe" || prior.body["requestHash"] != request_hash {
            return Err(WorkbenchError::conflict(
                "Figure operation was reused with changed arguments",
            ));
        }
        return Ok(prior);
    }
    let entries = resolve(store, &r.workspace_id, &r.spec)?;
    let profile = research::get_execution_profile(store, &r.python_profile_id)?;
    if profile.workspace_id != r.workspace_id
        || profile.adapter != "command"
        || !Path::new(&profile.argv[0])
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
            .starts_with("python")
    {
        return Err(WorkbenchError::invalid(
            "Select this project's Python execution profile with matplotlib installed",
        ));
    }
    let identity = desk::hash(format!("{}:{}", r.workspace_id, r.operation_id).as_bytes());
    let dir = project::program_directory(store, &r.workspace_id, &identity)?;
    let mut spec = serde_json::to_value(&r.spec).map_err(err)?;
    spec["entries"] = json!(entries);
    for (path, bytes) in [
        ("plot.py", include_bytes!("plot.py").as_slice().to_vec()),
        (
            "figure.json",
            serde_json::to_vec_pretty(&spec).map_err(err)?,
        ),
    ] {
        project::write_program_input(&dir, path, &bytes)?;
    }
    let p = save_profile_once(
        store,
        research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: r.workspace_id.clone(),
            name: crate::workbench::search::prefix(&format!("Figure: {}", r.spec.title), 300)
                .into(),
            adapter: "command".into(),
            argv: vec![profile.argv[0].clone(), "plot.py".into()],
            cwd: dir.to_string_lossy().into_owned(),
            environment: profile.environment,
            inputs: vec!["plot.py".into(), "figure.json".into()],
            timeout_seconds: 120,
            outputs: vec![
                "figure.svg".into(),
                "figure.pdf".into(),
                "figure.png".into(),
                "figure-receipt.json".into(),
            ],
            expected_revision: None,
            operation_id: format!("{}-profile", r.operation_id),
        },
    )?;
    let plan = research::execution_plan::capture(
        store,
        research::execution_plan::CapturePlanRequest {
            profile_id: p.id,
            parameters: json!({}),
            random_seed: None,
            toolchain_version:
                "Selected Python environment; matplotlib version reported by figure-receipt.json"
                    .into(),
            research_inputs: r.spec.entries.iter().map(|e| e.source.clone()).collect(),
            operation_id: format!("{}-capture", r.operation_id),
        },
    )?;
    desk::insert(
        store,
        &r.workspace_id,
        "figure_recipe",
        &r.spec.title,
        json!({"spec":r.spec,"entries":entries,"planId":plan.id,"sources":r.spec.entries.iter().map(|e|&e.source).collect::<Vec<_>>(),"adapter":"pipeline-matplotlib-v1","requestHash":request_hash}),
        None,
        &r.operation_id,
    )
}
pub fn adopt_figure(
    store: &Store,
    ws: &str,
    recipe_id: &str,
    execution_id: &str,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    let recipe = desk::record(store, ws, recipe_id)?;
    if recipe.kind != "figure_recipe" {
        return Err(WorkbenchError::invalid("Choose a figure recipe"));
    }
    let e = research::get_execution(store, execution_id)?;
    if e.workspace_id != ws
        || e.outcome != "completed"
        || e.input_manifest["planId"] != recipe.body["planId"]
    {
        return Err(WorkbenchError::invalid(
            "Choose a successful execution of this exact figure plan",
        ));
    }
    let mut artifacts = Vec::new();
    for (name, extension) in [
        ("figure.svg", "svg"),
        ("figure.pdf", "pdf"),
        ("figure.png", "png"),
        ("figure-receipt.json", "json"),
    ] {
        let a = e.output_manifest["artifacts"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["path"] == name)
            .ok_or_else(|| {
                WorkbenchError::invalid(
                    "The figure execution is missing an expected adopted output",
                )
            })?;
        let id = a["artifactId"]
            .as_str()
            .ok_or_else(|| WorkbenchError::invalid("Figure output lacks an artifact identity"))?;
        let bytes = artifact_bytes(store, ws, id, 8 * 1024 * 1024)?;
        artifacts.push(blob(store, ws, &bytes, extension)?);
    }
    let spec: AssetSpec = serde_json::from_value(recipe.body["spec"].clone()).map_err(err)?;
    let asset = PublicationAsset {
        style_version: 1,
        sources: spec.entries.iter().map(|e| e.source.clone()).collect(),
        spec,
        entries: serde_json::from_value(recipe.body["entries"].clone()).map_err(err)?,
        artifacts,
        execution_id: Some(e.id),
    };
    desk::insert(
        store,
        ws,
        "publication_asset",
        &recipe.title,
        serde_json::to_value(asset).map_err(err)?,
        None,
        operation,
    )
}
pub fn stage(
    store: &Store,
    ws: &str,
    asset_id: &str,
    artifact_id: &str,
    checkpoint: &str,
    path: &str,
    expected: Option<&str>,
) -> WorkbenchResult<Value> {
    let r = desk::record(store, ws, asset_id)?;
    let artifacts = r.body["artifacts"]
        .as_array()
        .ok_or_else(|| WorkbenchError::invalid("Choose a generated publication or deliverable"))?;
    if !artifacts.iter().any(|a| a["id"] == artifact_id) {
        return Err(WorkbenchError::invalid(
            "Artifact does not belong to this asset",
        ));
    }
    let bytes = artifact_bytes(store, ws, artifact_id, 8 * 1024 * 1024)?;
    let hash = desk::hash(&bytes);
    let mut files = BTreeMap::from([(path.to_owned(), (expected.map(str::to_owned), bytes))]);
    for attachment in r.body["attachments"].as_array().into_iter().flatten() {
        let a: Artifact = serde_json::from_value(attachment["artifact"].clone()).map_err(err)?;
        let relative = attachment["path"]
            .as_str()
            .ok_or_else(|| WorkbenchError::invalid("Deliverable attachment path is missing"))?;
        let destination = Path::new(path)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(relative)
            .to_string_lossy()
            .into_owned();
        files.insert(
            destination,
            (None, artifact_bytes(store, ws, &a.id, 8 * 1024 * 1024)?),
        );
    }
    let result = project::stage_publication_files(
        store,
        ws,
        checkpoint,
        files
            .into_iter()
            .map(|(path, (expected, bytes))| (path, expected, bytes))
            .collect(),
    )?;
    let operation = format!(
        "inclusion-{}",
        desk::hash(format!("{}:{checkpoint}:{path}:{hash}", r.id).as_bytes())
    );
    desk::insert(
        store,
        ws,
        "asset_inclusion",
        crate::workbench::search::prefix(
            &format!(
                "{} in {}",
                r.title,
                Path::new(path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
            500,
        ),
        json!({"sources":[r.reference()],"checkpointId":checkpoint,"path":path,"hash":hash,"stagedFiles":result["files"],"acceptance":"Inspect the checkpoint application and current accepted hashes; staging itself is not acceptance"}),
        None,
        &operation,
    )?;
    Ok(result)
}
