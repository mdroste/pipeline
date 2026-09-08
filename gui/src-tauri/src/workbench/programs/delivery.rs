use super::*;
use crate::workbench::{project, research, search};
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Kit {
    pub id: &'static str,
    pub version: u32,
    pub title: &'static str,
    pub instructions: &'static str,
    pub sections: Vec<&'static str>,
    pub tasks: Vec<&'static str>,
    pub recipe_suggestions: Vec<&'static str>,
}
pub fn kits() -> Vec<Kit> {
    [
    ("literature","Literature synthesis","Frame the question and distinguish evidence from interpretation.",vec!["Question and scope","Search log","Evidence synthesis"],vec!["Define inclusion criteria","Compare the closest papers"],vec!["literature synthesis"]),
    ("empirical","Empirical research","Record the estimand, sample, measurement, and identifying assumptions before estimation.",vec!["Research question","Data and sample","Identification and robustness"],vec!["Document data provenance and access policy","Specify the baseline and one discriminating robustness check"],vec!["data audit"]),
    ("quantitative","Quantitative model","Record equilibrium objects, calibration targets, solution tolerances, and units.",vec!["Model and equilibrium","Calibration targets","Numerical experiments"],vec!["Check accounting identities and limiting cases","Define moments and shock normalization"],vec!["limiting cases","accounting identities"]),
    ("theory","Theory note","Separate assumptions, propositions, proof steps, and numerical evidence.",vec!["Question and mechanism","Assumptions and notation","Propositions and unresolved steps"],vec!["Write the minimal proposition","Identify a boundary case or counterexample search"],vec!["comparative statics"]),
    ("grant","Grant proposal","Connect the question to the proposed work, feasibility, resources, and deliverables.",vec!["Motivation and aims","Research approach","Work plan and deliverables"],vec!["Define aims and success criteria","Identify feasibility evidence and resource constraints"],vec!["Grant Proposal Review"]),
    ("seminar","Seminar presentation","Build the argument around the research question, mechanism, evidence, and implications.",vec!["Question and motivation","Mechanism and evidence","Implications and open questions"],vec!["Select the central figure and its exact source","Write a talk outline with timing"],vec![]),
    ("teaching","Teaching preparation","State learning objectives and distinguish definitions, worked examples, and exercises.",vec!["Learning objectives","Definitions and worked examples","Exercises and assessment"],vec!["Choose a worked example with verified calculations","Draft an exercise and solution outline"],vec![]),
].into_iter().map(|(id,title,instructions,sections,tasks,recipe_suggestions)|Kit{id,version:1,title,instructions,sections,tasks,recipe_suggestions}).collect()
}
/// Ordinary notes, with a deterministic operation receipt for partial-install recovery.
pub fn note(
    store: &Store,
    ws: &str,
    kind: &str,
    body: &str,
    operation: &str,
) -> WorkbenchResult<research::ResearchNote> {
    let body = body.trim();
    let prior:Option<String>=store.connection()?.query_row("SELECT entity_id FROM change_log WHERE operation_id=?1 AND entity_type='research_note' AND workspace_id=?2",params![operation,ws],|r|r.get(0)).optional().map_err(err)?;
    if let Some(id) = prior {
        let n = research::get_note(store, &id)?;
        if n.body != body || n.kind != kind {
            return Err(WorkbenchError::conflict(
                "This installation operation already has different note content",
            ));
        }
        return Ok(n);
    }
    research::create_note(
        store,
        research::CreateNoteRequest {
            workspace_id: ws.into(),
            paper_id: None,
            kind: kind.into(),
            body: body.into(),
            state: Some("proposed".into()),
            origin: "research kit / researcher".into(),
            pinned: false,
            operation_id: operation.into(),
        },
        false,
    )
}
pub fn install(
    store: &Store,
    ws: &str,
    kit_id: &str,
    instructions: &str,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    let kit = kits()
        .into_iter()
        .find(|k| k.id == kit_id)
        .ok_or_else(|| WorkbenchError::invalid("Choose an available research kit"))?;
    bounded(instructions, 16000)?;
    let body = format!(
        "# {}\n\n{}\n\n{}",
        kit.title,
        instructions,
        kit.sections
            .iter()
            .map(|s| format!("## {s}\n"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let n = note(
        store,
        ws,
        "next_step",
        &body,
        &format!("{operation}-outline"),
    )?;
    let mut tasks = Vec::new();
    for (i, objective) in kit.tasks.iter().enumerate() {
        tasks.push(project::mutate(
            store,
            project::ProjectMutation {
                workspace_id: ws.into(),
                operation_id: format!("{operation}-task-{i}"),
                action: project::ProjectAction::CreateTask {
                    objective: (*objective).into(),
                    anchor_id: None,
                    expected_outputs: vec![],
                    expected_checks: vec![
                        "Researcher reviews the evidence and records remaining limitations".into(),
                    ],
                },
            },
        )?);
    }
    desk::insert(
        store,
        ws,
        "kit_install",
        kit.title,
        json!({"kit":kit,"instructions":instructions,"noteId":n.id,"tasks":tasks,"softwareInstalled":false,"executionAuthorized":false}),
        None,
        operation,
    )
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Section {
    pub heading: String,
    pub text: String,
    pub sources: Vec<ResearchObjectRef>,
    pub assets: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Outline {
    pub title: String,
    pub template: String,
    pub sections: Vec<Section>,
}
pub fn assemble(
    store: &Store,
    ws: &str,
    o: Outline,
    supersedes: Option<&str>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    bounded(&o.title, 300)?;
    if !["manuscript", "memo", "seminar", "grant", "teaching"].contains(&o.template.as_str())
        || o.sections.is_empty()
        || o.sections.len() > 30
    {
        return Err(WorkbenchError::invalid(
            "Choose a deliverable template and 1–30 sections",
        ));
    }
    let mut md = format!("# {}\n\n", o.title);
    let mut latex = format!(
        "% Pipeline deliverable template v1: {}\n\\section*{{{}}}\n",
        o.template,
        super::assets::tex(&o.title)
    );
    let mut sources = Vec::new();
    let mut attached = Vec::new();
    for section in &o.sections {
        bounded(&section.heading, 300)?;
        if section.text.len() > 16000 || section.assets.len() > 10 {
            return Err(WorkbenchError::invalid(
                "Deliverable section exceeds its limits",
            ));
        }
        refs(store, ws, &section.sources, 12)?;
        md.push_str(&format!("## {}\n\n{}\n\n", section.heading, section.text));
        latex.push_str(&format!(
            "\\subsection*{{{}}}\n{}\n\n",
            super::assets::tex(&section.heading),
            super::assets::tex(&section.text)
        ));
        for source in &section.sources {
            let v = search::read_object(store, ws, source, 8000)?;
            md.push_str(&format!(
                "Source excerpt ({} @ {}):\n\n{}\n\n",
                v.title,
                source.revision,
                v.text
                    .lines()
                    .map(|l| format!("> {l}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
            latex.push_str(&format!(
                "\\paragraph{{Source: {}}}\\begin{{quote}}{}\\end{{quote}}\n",
                super::assets::tex(&v.title),
                super::assets::tex(&v.text)
            ));
            sources.push(source.clone());
        }
        for id in &section.assets {
            let (asset, p): (_, super::assets::PublicationAsset) =
                load(store, ws, id, "publication_asset")?;
            sources.push(asset.reference());
            for a in &p.artifacts {
                if ["md", "tex", "png", "pdf"].contains(&a.extension.as_str()) {
                    let name = format!("assets/{}.{}", a.id, a.extension);
                    if a.extension == "md" {
                        md.push_str(
                            &String::from_utf8(artifact_bytes(store, ws, &a.id, 8 * 1024 * 1024)?)
                                .map_err(err)?,
                        );
                    }
                    if a.extension == "tex" {
                        latex.push_str(
                            &String::from_utf8(artifact_bytes(store, ws, &a.id, 8 * 1024 * 1024)?)
                                .map_err(err)?,
                        );
                    }
                    if a.extension == "png" {
                        md.push_str(&format!("\n![{}]({name})\n", asset.title));
                    }
                    if a.extension == "pdf" {
                        latex.push_str(&format!("\n\\begin{{figure}}[htbp]\\centering\\includegraphics[width=0.95\\linewidth]{{{name}}}\\caption{{{}}}\\end{{figure}}\n",super::assets::tex(&asset.title)));
                    }
                    attached.push(json!({"path":name,"artifact":a}));
                }
            }
        }
    }
    let artifacts = vec![
        blob(store, ws, md.as_bytes(), "md")?,
        blob(store, ws, latex.as_bytes(), "tex")?,
    ];
    desk::insert(
        store,
        ws,
        "deliverable",
        &o.title,
        json!({"templateVersion":1,"outline":o,"sources":sources,"artifacts":artifacts,"attachments":attached,"state":"draft","texRequirements":["graphicx"],"regeneration":"New immutable draft; stage into a task copy to compare and accept"}),
        supersedes,
        operation,
    )
}
pub fn role(
    store: &Store,
    ws: &str,
    role: &str,
    object: ResearchObjectRef,
    supersedes: Option<&str>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    if !["main", "grant", "seminar", "teaching", "memo", "supporting"].contains(&role) {
        return Err(WorkbenchError::invalid("Unknown deliverable role"));
    }
    exact(store, ws, &object)?;
    desk::insert(
        store,
        ws,
        "deliverable_role",
        role,
        json!({"role":role,"object":object,"sources":[object]}),
        supersedes,
        operation,
    )
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoauthorBrief {
    pub questions: Vec<String>,
    pub change_summary: String,
    pub excluded_material: String,
    pub sources: Vec<ResearchObjectRef>,
    pub comments: Vec<ReviewComment>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewComment {
    pub source: ResearchObjectRef,
    pub comment: String,
    pub decision: String,
}
pub fn coauthor(
    store: &Store,
    ws: &str,
    title: &str,
    b: CoauthorBrief,
    supersedes: Option<&str>,
    operation: &str,
) -> WorkbenchResult<DeskRecord> {
    bounded(&b.change_summary, 8000)?;
    bounded(&b.excluded_material, 8000)?;
    refs(store, ws, &b.sources, 32)?;
    if b.questions.len() > 20 || b.comments.len() > 50 {
        return Err(WorkbenchError::invalid("Coauthor brief exceeds its limits"));
    }
    for q in &b.questions {
        bounded(q, 2000)?;
    }
    for c in &b.comments {
        exact(store, ws, &c.source)?;
        bounded(&c.comment, 4000)?;
        if c.decision.len() > 4000 {
            return Err(WorkbenchError::invalid(
                "Coauthor decision exceeds its limit",
            ));
        }
    }
    let mut readable = format!(
        "# {title}\n\nChanges: {}\n\nExcluded material: {}\n\nQuestions:\n{}\n\n",
        b.change_summary,
        b.excluded_material,
        b.questions
            .iter()
            .map(|q| format!("- {q}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    for source in &b.sources {
        let v = search::read_object(store, ws, source, 4000)?;
        readable.push_str(&format!(
            "## {}\nExact source: {} {} @ {}\n\n{}\n\n",
            v.title, source.kind, source.id, source.revision, v.text
        ));
    }
    readable.push_str(&format!(
        "## Revision-specific comments and decisions\n\n{}\n",
        serde_json::to_string_pretty(&b.comments).map_err(err)?
    ));
    let n = note(
        store,
        ws,
        "handoff",
        &readable,
        &format!("{operation}-brief"),
    )?;
    let artifact = blob(store, ws, readable.as_bytes(), "md")?;
    desk::insert(
        store,
        ws,
        "coauthor_review",
        title,
        json!({"brief":b,"sources":b.sources,"noteId":n.id,"artifacts":[artifact],"exchangeNotice":"Include notes in the existing .pwex preview. This readable handoff preserves source identities and comments; selected source objects still require their normal exchange selections."}),
        supersedes,
        operation,
    )
}
pub fn export_bundle(store: &Store, ws: &str, id: &str, path: &str) -> WorkbenchResult<Value> {
    let r = desk::record(store, ws, id)?;
    if r.kind != "deliverable" || !Path::new(path).is_absolute() {
        return Err(WorkbenchError::invalid(
            "Choose a deliverable and absolute export path",
        ));
    }
    let mut files = BTreeMap::new();
    for a in r.body["artifacts"].as_array().into_iter().flatten() {
        let a: Artifact = serde_json::from_value(a.clone()).map_err(err)?;
        files.insert(
            format!("deliverable.{}", a.extension),
            artifact_bytes(store, ws, &a.id, 8 * 1024 * 1024)?,
        );
    }
    for a in r.body["attachments"].as_array().into_iter().flatten() {
        let file = a["path"]
            .as_str()
            .ok_or_else(|| WorkbenchError::invalid("Missing asset filename"))?;
        let a: Artifact = serde_json::from_value(a["artifact"].clone()).map_err(err)?;
        files.insert(
            file.into(),
            artifact_bytes(store, ws, &a.id, 8 * 1024 * 1024)?,
        );
    }
    files.insert(
        "sources.json".into(),
        serde_json::to_vec_pretty(&r.body).map_err(err)?,
    );
    files.insert("README.md".into(),b"# Pipeline deliverable draft\n\nThe Markdown and TeX drafts refer to the included assets directory. TeX output is a document fragment; include graphicx in the enclosing document. Exact source references and template version are in sources.json. Compare and review before accepting this draft.\n".to_vec());
    if files.values().map(Vec::len).sum::<usize>() > 64 * 1024 * 1024 {
        return Err(WorkbenchError::invalid("Deliverable export exceeds 64 MiB"));
    }
    let path = Path::new(path);
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(err)?;
    {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(temp.as_file_mut());
        for (path, bytes) in files {
            zip.start_file(
                path,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .unix_permissions(0o600),
            )
            .map_err(err)?;
            zip.write_all(&bytes).map_err(err)?;
        }
        zip.finish().map_err(err)?;
    }
    temp.as_file().sync_all().map_err(err)?;
    temp.persist(path).map_err(|e| err(e.error))?;
    Ok(json!({"path":path}))
}
