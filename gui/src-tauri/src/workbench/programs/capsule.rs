//! Independent inert replication format. It never imports execution authorization.
use super::*;
use crate::workbench::{data, project, research};
use std::io::Write;
const FORMAT: &str = "research-capsule-v1";
const MAX_TOTAL: usize = 64 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Expected {
    pub output: String,
    pub pointer: String,
    pub value: f64,
    pub absolute_tolerance: f64,
    pub relative_tolerance: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub path: String,
    pub hash: String,
    pub included: bool,
    pub external_requirement: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Environment {
    pub variables: BTreeMap<String, String>,
    pub external_variables: Vec<String>,
    pub platform: String,
    pub lockfiles: Vec<String>,
    pub coverage: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub environment: Environment,
    pub label: String,
    pub inputs: Vec<Input>,
    pub arguments: Vec<String>,
    pub outputs: Vec<String>,
    pub parameters: Value,
    pub random_seed: Option<String>,
    pub timeout_seconds: u64,
    pub toolchain: String,
    pub expected: Vec<Expected>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub title: String,
    pub plans: Vec<Plan>,
    pub limitations: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub plan_id: String,
    pub included_paths: Vec<String>,
    pub external_requirements: BTreeMap<String, String>,
    pub expected: Vec<Expected>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Export {
    pub workspace_id: String,
    pub title: String,
    pub selections: Vec<Selection>,
    pub path: String,
}
fn relative(path: &str) -> WorkbenchResult<()> {
    if path.is_empty()
        || path.len() > 1000
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || Path::new(path)
            .components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
        || path
            .split('/')
            .any(|s| s.is_empty() || s == ".git" || s.starts_with(".pipeline"))
    {
        return Err(WorkbenchError::invalid(
            "Capsule paths must be normalized relative paths outside metadata",
        ));
    }
    Ok(())
}
fn validate(m: &Manifest) -> WorkbenchResult<()> {
    bounded(&m.title, 300)?;
    if m.format != FORMAT
        || m.version != 1
        || m.plans.is_empty()
        || m.plans.len() > 16
        || m.limitations.len() > 8000
    {
        return Err(WorkbenchError::invalid(
            "Unsupported or oversized research capsule manifest",
        ));
    }
    for p in &m.plans {
        bounded(&p.label, 300)?;
        if p.inputs.is_empty()
            || p.inputs.len() > 100
            || p.arguments.is_empty()
            || p.arguments.len() > 64
            || p.outputs.len() > 32
            || p.expected.len() > 100
            || p.timeout_seconds == 0
            || p.timeout_seconds > 3600
            || p.parameters.to_string().len() > 16000
        {
            return Err(WorkbenchError::invalid(
                "Capsule plan exceeds its declared limits",
            ));
        }
        if p.environment.variables.len() + p.environment.external_variables.len() > 100
            || p.environment.coverage.len() > 4000
            || p.environment.platform.len() > 200
        {
            return Err(WorkbenchError::invalid(
                "Capsule environment exceeds its limits",
            ));
        }
        for (name, value) in &p.environment.variables {
            if name.len() > 200
                || value.len() > 1000
                || name.chars().any(char::is_control)
                || value.contains(['/', '\\'])
            {
                return Err(WorkbenchError::invalid(
                    "Capsule environment must use portable declared values",
                ));
            }
        }
        for name in &p.environment.external_variables {
            bounded(name, 200)?;
        }
        for path in &p.environment.lockfiles {
            relative(path)?;
        }
        let mut names = std::collections::HashSet::new();
        for i in &p.inputs {
            relative(&i.path)?;
            if !names.insert(&i.path)
                || i.hash.len() != 64
                || !i.hash.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(WorkbenchError::invalid(
                    "Duplicate capsule input or invalid hash",
                ));
            }
            if !i.included {
                bounded(i.external_requirement.as_deref().unwrap_or(""), 4000)?;
            }
        }
        for a in &p.arguments {
            if a.len() > 4000 || a.contains('\0') || Path::new(a).is_absolute() {
                return Err(WorkbenchError::invalid(
                    "Capsule arguments contain an exporting-machine path",
                ));
            }
        }
        for o in &p.outputs {
            relative(o)?;
        }
        for e in &p.expected {
            if !p.outputs.contains(&e.output)
                || !e.value.is_finite()
                || !e.absolute_tolerance.is_finite()
                || !e.relative_tolerance.is_finite()
                || e.absolute_tolerance < 0.0
                || e.relative_tolerance < 0.0
                || e.pointer.len() > 1000
                || (!e.pointer.is_empty() && !e.pointer.starts_with('/'))
            {
                return Err(WorkbenchError::invalid(
                    "Invalid output, JSON pointer or numeric tolerance",
                ));
            }
        }
    }
    Ok(())
}
fn assemble(store: &Store, r: &Export) -> WorkbenchResult<(Manifest, BTreeMap<String, Vec<u8>>)> {
    if r.selections.is_empty() || r.selections.len() > 16 {
        return Err(WorkbenchError::invalid(
            "Select 1–16 captured execution plans",
        ));
    }
    let mut plans = Vec::new();
    let mut blobs = BTreeMap::new();
    let policy = data::policy(store, &r.workspace_id)?;
    for s in &r.selections {
        let (record, p): (_, research::execution_plan::ExecutionPlan) =
            load(store, &r.workspace_id, &s.plan_id, "execution_plan")?;
        if p.profile.adapter != "command" {
            return Err(WorkbenchError::invalid("Capsule v1 currently supports configured command profiles. Licensed and specialized adapters need local setup outside the capsule."));
        }
        if s.included_paths
            .iter()
            .any(|path| !p.captured_files.iter().any(|f| &f.path == path))
        {
            return Err(WorkbenchError::invalid("Select only captured inputs"));
        }
        let mut inputs = Vec::new();
        for f in &p.captured_files {
            let included = s.included_paths.contains(&f.path);
            let ext = Path::new(&f.path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if included
                && !policy.package_data
                && !["py", "r", "jl", "do", "tex", "bib"].contains(&ext.as_str())
            {
                return Err(WorkbenchError::invalid("Enable the project's package-data policy to include non-script inputs; otherwise declare them external"));
            }
            if included {
                blobs.insert(
                    f.hash.clone(),
                    artifact_bytes(store, &r.workspace_id, &f.artifact_id, 8 * 1024 * 1024)?,
                );
            }
            inputs.push(Input {
                path: f.path.clone(),
                hash: f.hash.clone(),
                included,
                external_requirement: s.external_requirements.get(&f.path).cloned(),
            });
        }
        let mut variables = BTreeMap::new();
        let mut external_variables = Vec::new();
        for (name, value) in p.profile.environment.as_object().into_iter().flatten() {
            if name.starts_with("PIPELINE_") {
                continue;
            }
            let value = value.as_str().unwrap_or("");
            if value.len() > 1000 || value.contains(['/', '\\']) {
                external_variables.push(name.clone());
            } else {
                variables.insert(name.clone(), value.into());
            }
        }
        let environment=Environment{variables,external_variables,platform:p.platform.clone(),lockfiles:p.environment["lockfiles"].as_array().into_iter().flatten().filter_map(|v|v.as_str().map(str::to_owned)).collect(),coverage:"Declared variables and lockfile identities only; inherited host variables and installed packages are not captured. Machine paths require recipient-local values. Review toolchain and package requirements before authorization.".into()};
        plans.push(Plan {
            environment,
            label: record.title,
            inputs,
            arguments: p.profile.argv.into_iter().skip(1).collect(),
            outputs: p.profile.outputs,
            parameters: p.parameters,
            random_seed: p.random_seed,
            timeout_seconds: p.profile.timeout_seconds,
            toolchain: p.toolchain_version,
            expected: s.expected.clone(),
        });
    }
    let m=Manifest{format:FORMAT.into(),version:1,title:r.title.clone(),plans,limitations:"Declared inputs and expected outputs only. Recipient supplies a local toolchain, environment and any external inputs, then captures, authorizes and tests a new plan. Numeric agreement is not scientific validity. Credentials, original absolute roots and execution grants are excluded.".into()};
    validate(&m)?;
    if blobs.values().map(Vec::len).sum::<usize>() > MAX_TOTAL {
        return Err(WorkbenchError::invalid(
            "Selected capsule inputs exceed 64 MiB",
        ));
    }
    Ok((m, blobs))
}
pub fn preview(store: &Store, r: &Export) -> WorkbenchResult<Value> {
    let (m, b) = assemble(store, r)?;
    Ok(
        json!({"manifest":m,"includedBytes":b.values().map(Vec::len).sum::<usize>(),"fingerprint":desk::hash(&serde_json::to_vec(&m).map_err(err)?)}),
    )
}
pub fn export(store: &Store, r: Export, fingerprint: &str) -> WorkbenchResult<Value> {
    let (m, blobs) = assemble(store, &r)?;
    let encoded = serde_json::to_vec_pretty(&m).map_err(err)?;
    let digest = desk::hash(&serde_json::to_vec(&m).map_err(err)?);
    if digest != fingerprint {
        return Err(WorkbenchError::conflict(
            "Capsule selection changed since preview",
        ));
    }
    if !Path::new(&r.path).is_absolute() {
        return Err(WorkbenchError::invalid(
            "Choose an absolute capsule export path",
        ));
    }
    let path = Path::new(&r.path);
    let mut temp = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| WorkbenchError::invalid("Choose an export directory"))?,
    )
    .map_err(err)?;
    {
        let mut zip = zip::ZipWriter::new(temp.as_file_mut());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o600);
        zip.start_file("manifest.json", options).map_err(err)?;
        zip.write_all(&encoded).map_err(err)?;
        zip.start_file("README.md", options).map_err(err)?;
        let mut readme = format!("# {}\n\n{}\n\n", m.title, m.limitations);
        for (i, p) in m.plans.iter().enumerate() {
            readme.push_str(&format!(
                "## {}. {}\nToolchain: {}\nExpected outputs: {}\n",
                i + 1,
                p.label,
                p.toolchain,
                p.outputs.join(", ")
            ));
            for input in &p.inputs {
                readme.push_str(&format!(
                    "- {}: {} ({})\n",
                    input.path,
                    if input.included {
                        "included"
                    } else {
                        "external"
                    },
                    input.external_requirement.as_deref().unwrap_or(&input.hash)
                ));
            }
        }
        zip.write_all(readme.as_bytes()).map_err(err)?;
        for (hash, bytes) in blobs {
            zip.start_file(format!("blobs/{hash}"), options)
                .map_err(err)?;
            zip.write_all(&bytes).map_err(err)?;
        }
        zip.finish().map_err(err)?;
    }
    temp.as_file_mut().sync_all().map_err(err)?;
    temp.persist(path).map_err(|e| err(e.error))?;
    Ok(json!({"path":r.path,"fingerprint":digest,"manifest":m}))
}
fn read(path: &str) -> WorkbenchResult<(Manifest, BTreeMap<String, Vec<u8>>)> {
    let file = fs::File::open(path).map_err(err)?;
    if file.metadata().map_err(err)?.len() > MAX_TOTAL as u64 + 1024 * 1024 {
        return Err(WorkbenchError::invalid("Capsule exceeds its archive limit"));
    }
    let mut zip = zip::ZipArchive::new(file).map_err(err)?;
    if zip.len() > 1602 {
        return Err(WorkbenchError::invalid("Too many capsule entries"));
    }
    let mut entries = BTreeMap::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(err)?;
        let name = entry.name().to_owned();
        relative(&name)?;
        if !(name == "manifest.json" || name == "README.md" || name.starts_with("blobs/"))
            || entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            || entry.size() > 8 * 1024 * 1024
        {
            return Err(WorkbenchError::invalid("Unsupported capsule entry"));
        }
        let mut bytes = Vec::new();
        (&mut entry)
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        total += bytes.len();
        if bytes.len() > 8 * 1024 * 1024
            || total > MAX_TOTAL + 1024 * 1024
            || entries.insert(name, bytes).is_some()
        {
            return Err(WorkbenchError::invalid(
                "Duplicate or oversized capsule content",
            ));
        }
    }
    let manifest = entries
        .remove("manifest.json")
        .ok_or_else(|| WorkbenchError::invalid("Capsule manifest is missing"))?;
    if manifest.len() > 512 * 1024 {
        return Err(WorkbenchError::invalid("Capsule manifest exceeds 512 KiB"));
    }
    let m: Manifest = serde_json::from_slice(&manifest).map_err(err)?;
    validate(&m)?;
    entries.remove("README.md");
    let mut blobs = BTreeMap::new();
    for p in &m.plans {
        for input in &p.inputs {
            if input.included {
                let bytes = entries
                    .get(&format!("blobs/{}", input.hash))
                    .ok_or_else(|| {
                        WorkbenchError::invalid(format!("Missing included input: {}", input.path))
                    })?;
                if desk::hash(bytes) != input.hash {
                    return Err(WorkbenchError::invalid("Capsule input hash mismatch"));
                }
                blobs.insert(input.hash.clone(), bytes.clone());
            }
        }
    }
    if entries.len() != blobs.len() {
        return Err(WorkbenchError::invalid("Capsule contains undeclared blobs"));
    }
    Ok((m, blobs))
}
pub fn inspect(path: &str) -> WorkbenchResult<Value> {
    let (m, b) = read(path)?;
    Ok(
        json!({"manifest":m,"fingerprint":desk::hash(&serde_json::to_vec(&m).map_err(err)?),"includedBytes":b.values().map(Vec::len).sum::<usize>(),"executable":false}),
    )
}
pub fn import(
    store: &Store,
    ws: &str,
    path: &str,
    fingerprint: &str,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    let (m, bytes) = read(path)?;
    if desk::hash(&serde_json::to_vec(&m).map_err(err)?) != fingerprint {
        return Err(WorkbenchError::conflict("Capsule changed since inspection"));
    }
    let mut imported = BTreeMap::new();
    for (hash, bytes) in bytes {
        imported.insert(hash, blob(store, ws, &bytes, "bin")?);
    }
    desk::insert(
        store,
        ws,
        "capsule_import",
        &m.title,
        json!({"manifest":m,"inputs":imported,"authority":"inert","fingerprint":fingerprint}),
        None,
        operation,
    )
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Bind {
    pub workspace_id: String,
    pub capsule_id: String,
    pub ordinal: usize,
    pub local_profile_id: String,
    pub external_inputs: BTreeMap<String, String>,
    pub operation_id: String,
}
pub fn bind(store: &Store, r: Bind) -> WorkbenchResult<Value> {
    bounded(&r.operation_id, 150)?;
    let request_hash = desk::hash(&serde_json::to_vec(&r).map_err(err)?);
    if let Some(prior) = desk::operation_record(store, &r.workspace_id, &r.operation_id)? {
        if prior.kind != "capsule_import" || prior.body["requestHash"] != request_hash {
            return Err(WorkbenchError::conflict(
                "Capsule binding operation was reused with different arguments",
            ));
        }
        let plan = desk::record(
            store,
            &r.workspace_id,
            prior.body["planId"]
                .as_str()
                .ok_or_else(|| WorkbenchError::invalid("Binding lacks a plan"))?,
        )?;
        return Ok(json!({"status":"prepared","binding":prior,"plan":plan,"authorized":false}));
    }
    let (record, m): (DeskRecord, Value) =
        load(store, &r.workspace_id, &r.capsule_id, "capsule_import")?;
    let manifest: Manifest = serde_json::from_value(m["manifest"].clone()).map_err(err)?;
    let p = manifest
        .plans
        .get(r.ordinal)
        .ok_or_else(|| WorkbenchError::invalid("Unknown capsule plan"))?;
    let local = research::get_execution_profile(store, &r.local_profile_id)?;
    if local.workspace_id != r.workspace_id || local.adapter != "command" {
        return Err(WorkbenchError::invalid(
            "Choose this project's local command toolchain",
        ));
    }
    let environment_mismatches=p.environment.variables.iter().filter(|(k,v)|local.environment[*k].as_str()!=Some(v.as_str())).map(|(k,v)|json!({"variable":k,"required":v})).chain(p.environment.external_variables.iter().filter(|k|local.environment[*k].as_str().is_none_or(str::is_empty)).map(|k|json!({"variable":k,"required":"A recipient-local value; exporting-machine path was excluded"}))).collect::<Vec<_>>();
    if !environment_mismatches.is_empty() {
        return Ok(
            json!({"status":"incomplete","environmentMismatches":environment_mismatches,"uncheckedOutputs":p.outputs}),
        );
    }
    let mut files = Vec::new();
    let mut missing = Vec::new();
    for input in &p.inputs {
        let bytes = if input.included {
            let a: Artifact =
                serde_json::from_value(m["inputs"][&input.hash].clone()).map_err(err)?;
            Some(artifact_bytes(
                store,
                &r.workspace_id,
                &a.id,
                8 * 1024 * 1024,
            )?)
        } else if let Some(id) = r.external_inputs.get(&input.path) {
            Some(artifact_bytes(store, &r.workspace_id, id, 8 * 1024 * 1024)?)
        } else {
            missing.push(input.path.clone());
            None
        };
        if let Some(bytes) = bytes {
            if desk::hash(&bytes) != input.hash {
                return Err(WorkbenchError::invalid(format!(
                    "External input hash differs: {}",
                    input.path
                )));
            }
            files.push((input.path.clone(), bytes));
        }
    }
    if !missing.is_empty() {
        return Ok(
            json!({"status":"incomplete","missingInputs":missing,"uncheckedOutputs":p.outputs}),
        );
    }
    let identity = desk::hash(format!("{}:{}", r.workspace_id, r.operation_id).as_bytes());
    let dir = project::program_directory(store, &r.workspace_id, &identity)?;
    for (path, bytes) in files {
        project::write_program_input(&dir, &path, &bytes)?;
    }
    let mut argv = vec![local.argv[0].clone()];
    argv.extend(p.arguments.clone());
    let profile = save_profile_once(
        store,
        research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: r.workspace_id.clone(),
            name: crate::workbench::search::prefix(&format!("Capsule: {}", p.label), 300).into(),
            adapter: "command".into(),
            argv,
            cwd: dir.to_string_lossy().into_owned(),
            environment: local.environment,
            inputs: p.inputs.iter().map(|i| i.path.clone()).collect(),
            outputs: p.outputs.clone(),
            timeout_seconds: p.timeout_seconds,
            expected_revision: None,
            operation_id: format!("{}-profile", r.operation_id),
        },
    )?;
    let plan = research::execution_plan::capture(
        store,
        research::execution_plan::CapturePlanRequest {
            research_inputs: vec![],
            profile_id: profile.id,
            parameters: p.parameters.clone(),
            random_seed: p.random_seed.clone(),
            toolchain_version: format!("Locally rebound capsule; requested {}", p.toolchain),
            operation_id: format!("{}-capture", r.operation_id),
        },
    )?;
    let binding = desk::insert(
        store,
        &r.workspace_id,
        "capsule_import",
        &format!("Bound: {}", p.label),
        json!({"sourceCapsuleId":record.id,"ordinal":r.ordinal,"planId":plan.id,"expected":p.expected,"outputs":p.outputs,"environment":p.environment,"requestHash":request_hash,"authority":"review_local_plan_before_authorizing"}),
        None,
        &r.operation_id,
    )?;
    Ok(json!({"status":"prepared","binding":binding,"plan":plan,"authorized":false}))
}
pub fn verify(store: &Store, ws: &str, binding: &str, execution: &str) -> WorkbenchResult<Value> {
    let r = desk::record(store, ws, binding)?;
    if r.kind != "capsule_import" || r.body["planId"].is_null() {
        return Err(WorkbenchError::invalid(
            "Choose a locally rebound capsule plan",
        ));
    }
    let e = research::get_execution(store, execution)?;
    if e.workspace_id != ws || e.input_manifest["planId"] != r.body["planId"] {
        return Err(WorkbenchError::invalid(
            "Execution is not a verification of this local capsule binding",
        ));
    }
    let expected: Vec<Expected> =
        serde_json::from_value(r.body["expected"].clone()).map_err(err)?;
    let mut checks = Vec::new();
    let has_numeric_checks = !expected.is_empty();
    let mut complete = e.outcome == "completed";
    let mut matches = true;
    for output in r.body["outputs"].as_array().into_iter().flatten() {
        if !e.output_manifest["artifacts"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|a| a["path"] == *output)
        {
            complete = false;
            checks.push(json!({"output":output,"status":"missing"}));
        }
    }
    for x in expected {
        let a = e.output_manifest["artifacts"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["path"] == x.output);
        let value = a
            .and_then(|a| a["artifactId"].as_str())
            .and_then(|id| artifact_bytes(store, ws, id, 8 * 1024 * 1024).ok())
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|v| v.pointer(&x.pointer).and_then(Value::as_f64));
        let matched = value.is_some_and(|v| {
            (v - x.value).abs() <= x.absolute_tolerance + x.relative_tolerance * x.value.abs()
        });
        if value.is_none() {
            complete = false;
        }
        if !matched {
            matches = false;
        }
        checks.push(json!({"expected":x,"actual":value,"status":if value.is_none(){"incomplete"}else if matched{"matched"}else{"mismatch"}}));
    }
    Ok(
        json!({"executionId":e.id,"executionOutcome":e.outcome,"status":if !complete{"incomplete"}else if matches&&has_numeric_checks{"checked"}else if matches{"executed_unchecked"}else{"mismatch"},"numericChecks":checks,"scientificValidity":"Not assessed","notice":"Successful execution and selected numeric checks do not establish scientific validity."}),
    )
}
