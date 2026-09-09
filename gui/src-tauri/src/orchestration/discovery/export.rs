use super::{
    super::{
        adapters,
        store::{err, Result, Store},
    },
    model::*,
};
use serde_json::{json, Value};
use std::io::{Cursor, Read, Write};

pub fn phase_name(p: &Phase) -> &'static str {
    match p {
        Phase::Orient => "Identify research standards",
        Phase::Acquire { .. } => "Inspect literature",
        Phase::Generate => "Generate projects",
        Phase::Screen { .. } => "Screen proposals",
        Phase::Assess { .. } => "Assess projects",
        Phase::ReviseProposal { .. } => "Revise project design",
        Phase::Shortlist => "Build shortlist",
        Phase::Select => "Choose projects",
        Phase::Research { .. } => "Investigate",
        Phase::Challenge { .. } => "Challenge findings",
        Phase::Draft { .. } => "Write or revise paper",
        Phase::Review { .. } => "Adversarial review",
        Phase::ExternalReview { .. } => "Structured Review",
        Phase::AssessPaper { .. } => "Assess final paper",
        Phase::Rank => "Rank papers",
        Phase::Deliver => "Deliver research",
    }
}
pub fn delivery(p: &Portfolio) -> Value {
    json!({"kind":"selfDiscovery","id":p.run.id,"mode":p.run.definition.mode,"requestedPapers":p.run.definition.paper_count,
        "ranking":p.run.ranking,"papers":p.papers.iter().map(|p|json!({"candidateId":p.candidate_id,"state":p.state,"reason":p.reason,
            "title":p.versions.last().map(|v|&v.manuscript.title),"abstract":p.versions.last().map(|v|&v.manuscript.abstract_text),
            "versionHash":p.versions.last().map(|v|&v.hash),"reviewCount":p.versions.last().map(|v|v.reviews.len()),
            "artifacts":p.versions.last().map(|v|&v.artifacts),"assessment":p.assessment})).collect::<Vec<_>>()})
}
pub fn view(p: &Portfolio) -> Value {
    json!({"id":p.run.id,"revision":p.run.revision,"definition":p.run.definition,"state":p.run.state,"reason":p.run.reason,"phase":p.run.phase,
        "phaseLabel":phase_name(&p.run.phase),"orientation":p.run.orientation,"candidateCount":p.candidates.len(),"selection":p.run.selection,
        "selectionHash":p.run.selection_hash,"selected":p.run.selected,"ranking":p.run.ranking,"actionsReserved":p.run.actions_reserved,
        "activeSeconds":p.run.active_seconds,"deadlineAt":p.run.deadline_at,"activeChild":p.run.active_child,"sourceSessionId":p.run.source_session_id,
        "workspaceId":p.run.workspace_id,"papers":p.papers.iter().map(|p|json!({"candidateId":p.candidate_id,"state":p.state,"reason":p.reason,"rounds":p.rounds.len(),
            "versions":p.versions.len(),"title":p.versions.last().map(|v|&v.manuscript.title),"assessment":p.assessment,
            "reviewCount":p.versions.last().map(|v|v.reviews.len()),"hash":p.versions.last().map(|v|&v.hash)})).collect::<Vec<_>>()})
}
pub fn report(p: &Portfolio) -> String {
    let mut s=format!("# {}\n\n{}\n\nStatus: {} — {}\n\nCandidates explored: {}. Papers requested: {}.\n\n## Final paper ranking\n",p.run.definition.mode.label(),p.run.definition.prompt,p.run.state,p.run.reason,p.candidates.len(),p.run.definition.paper_count);
    for rank in &p.run.ranking {
        if let Some(paper) = p
            .papers
            .iter()
            .find(|p| p.candidate_id == rank.candidate_id)
        {
            if let Some(v) = paper.versions.last() {
                s.push_str(&format!("\n### {}. {}\n\n{}\n\n{}\n\nUncertainty: {}\n\nState: {}. Version: {}. Hash: `{}`. Reviews: {}.\n",rank.rank,v.manuscript.title,v.manuscript.abstract_text,rank.reason,rank.uncertainty,paper.state,v.version,v.hash,v.reviews.len()));
            }
        }
    }
    if let Some(selection) = &p.run.selection {
        s.push_str(&format!(
            "\n## Project selection\n\n{}\n\nSelected IDs: {:?}\n",
            selection.rationale, p.run.selected
        ));
    }
    s.push_str("\n## Research coverage\n\nAll conclusions are model assessments. Exact artifact hashes identify reviewed versions; file capture alone does not verify a proof, observation, computation or interpretation. Papers with incomplete evidence or unavailable required work remain incomplete.\n");
    for paper in &p.papers {
        s.push_str(&format!(
            "\n- Project {}: {}. {}\n",
            paper.candidate_id, paper.state, paper.reason
        ));
    }
    s
}
pub fn bundle(s: &Store, p: &Portfolio) -> Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut total = 0usize;
    let mut add = |name: &str, bytes: &[u8]| -> Result<()> {
        total = total
            .checked_add(bytes.len())
            .ok_or("Export size overflow")?;
        if total > 256 * 1024 * 1024 {
            return Err("Research export exceeds 256 MiB".into());
        }
        zip.start_file(name, options).map_err(err)?;
        zip.write_all(bytes).map_err(err)
    };
    add("README.md", report(p).as_bytes())?;
    add("portfolio.json",&serde_json::to_vec_pretty(&json!({"contract":CONTRACT,"configuration":p.run.definition,"orientation":p.run.orientation,"candidates":p.candidates,"selection":p.run.selection,"selected":p.run.selected,"ranking":p.run.ranking,"literature":p.run.literature,"inputFiles":p.run.input_artifacts})).map_err(err)?)?;
    for (name, artifact) in &p.run.input_artifacts {
        let path = adapters::artifact_path(s, artifact)?;
        let mut bytes = Vec::new();
        crate::safety::open_regular_file(std::path::Path::new(&path))?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        add(&format!("inputs/{name}"), &bytes)?;
    }
    for paper in &p.papers {
        let prefix = format!("project-{}", paper.candidate_id);
        // Do not export native credentials, local execution grants or role scope settings.
        add(&format!("{prefix}/research.json"),&serde_json::to_vec_pretty(&json!({"rounds":paper.rounds,"challenges":paper.challenges,"assessment":paper.assessment,"state":paper.state,"evidence":paper.evidence,"versions":paper.versions.iter().map(|v|json!({"version":v.version,"hash":v.hash,"manuscript":v.manuscript,"artifacts":v.artifacts,"reviews":v.reviews,"externalReview":v.external_review,"responseToReview":v.manuscript.response_to_review})).collect::<Vec<_>>()})).map_err(err)?)?;
        if let Some(v) = paper.versions.last() {
            for (name, artifact) in &v.artifacts {
                if artifact["kind"] != "artifact" {
                    continue;
                }
                let path = adapters::artifact_path(s, artifact)?;
                let mut bytes = Vec::new();
                crate::safety::open_regular_file(std::path::Path::new(&path))?
                    .take(64 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(err)?;
                add(&format!("{prefix}/{name}"), &bytes)?;
            }
        }
        for (name, artifact) in &paper.evidence {
            if artifact["kind"] != "artifact" {
                continue;
            }
            let path = adapters::artifact_path(s, artifact)?;
            let mut bytes = Vec::new();
            crate::safety::open_regular_file(std::path::Path::new(&path))?
                .take(64 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(err)?;
            let filename = artifact["filename"].as_str().unwrap_or("evidence");
            add(&format!("{prefix}/evidence/{name}-{filename}"), &bytes)?;
        }
    }
    Ok(zip.finish().map_err(err)?.into_inner())
}
