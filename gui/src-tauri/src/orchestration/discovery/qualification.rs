//! Synthetic provider responses for deterministic tests and explicitly enabled disposable Tauri development runs.
use super::model::*;
#[cfg(all(feature = "e2e", debug_assertions))]
use super::{storage, store};
use serde_json::{json, Value};

#[cfg(all(feature = "e2e", debug_assertions))]
pub(crate) fn scripted(s: &store::Store, child: &store::TaskRun) -> store::Result<Option<Value>> {
    if std::env::var("PIPELINE_E2E_DISCOVERY").as_deref() != Ok("synthetic-history-fixture") {
        return Ok(None);
    }
    if std::env::var_os("PIPELINE_WORKBENCH_DEV_ROOT").is_none()
        || std::env::var_os("PIPELINE_E2E_TASK_ROOT").is_none()
    {
        return Err("Discovery qualification requires disposable stores".into());
    }
    let Some(id) = storage::owner(s, &child.id)? else {
        return Ok(None);
    };
    let p = storage::load(s, &id)?;
    let result = response(&p).to_string();
    Ok(Some(
        json!({"state":"completed","text":result,"finalText":result,"qualification":"synthetic-native-development"}),
    ))
}

pub(super) fn assessment(id: u32) -> Value {
    json!({"candidateId":id,"eligible":true,"contribution":4,"feasibility":4,"informationValue":3,"cluster":format!("cluster-{id}"),"duplicateOf":null,"strongestObjection":"The relation between sources remains uncertain","resolution":"Compare the preserved variants and provenance","uncertainty":"Incomplete archive coverage"})
}
pub(super) fn response(p: &Portfolio) -> Value {
    match p.run.phase {
        Phase::Orient => {
            json!({"fields":["History"],"subjectIds":[],"methodIds":[],"researchStandards":["Source criticism and provenance"],"contributionForms":["Interpretation supported by primary texts"],"proposalLenses":["Transmission","Attribution","Context"],"assumptions":[],"constraints":["Only available primary sources"],"literatureQueries":[]})
        }
        Phase::Generate => {
            json!({"proposals":(0..BATCH.min(p.run.definition.candidate_count-p.candidates.len())).map(|i|json!({"title":format!("Project {}",p.candidates.len()+i+1),"question":"What explains the textual variant?","contribution":"An account of source transmission","method":"Historical source criticism","firstTest":"Compare independently dated witnesses","requiredEvidence":["Primary texts"],"relatedWork":[],"risks":["Missing witness"]})).collect::<Vec<_>>()})
        }
        Phase::Screen { offset } => {
            json!({"assessments":p.candidates.iter().skip(offset).take(BATCH).map(|c|assessment(c.id)).collect::<Vec<_>>()})
        }
        Phase::Assess { index, .. } => {
            json!({"assessments":[assessment(p.run.deep_candidates[index])]})
        }
        Phase::ReviseProposal { index } => json!(
            p.candidates
                .iter()
                .find(|c| c.id == p.run.deep_candidates[index])
                .unwrap()
                .proposal
        ),
        Phase::Shortlist => {
            json!({"shortlist":p.run.deep_candidates.iter().take(p.run.definition.shortlist_count).collect::<Vec<_>>(),"recommended":p.run.deep_candidates.iter().take(p.run.definition.paper_count).collect::<Vec<_>>(),"rationale":"Distinct source questions with feasible comparisons"})
        }
        Phase::Research { .. } => {
            json!({"contract":"Assess transmission under explicit source limitations","summary":"The available witnesses support a bounded interpretation","claims":["The interpretation is provisional"],"limitations":["Missing witnesses prevent a definitive claim"],"sources":[],"files":[],"next":"draft","reason":"The bounded argument can be written with limitations"})
        }
        Phase::Challenge { .. } => {
            json!({"assessment":"The bounded argument supports a draft","unresolved":["Source coverage"],"progress":true,"next":"draft"})
        }
        Phase::Draft { paper } => {
            json!({"title":format!("Paper {}",p.papers[paper].candidate_id),"abstractText":"A bounded interpretation of source transmission","markdown":format!("# Paper {}\n\nA bounded argument. Version {}",p.papers[paper].candidate_id,p.papers[paper].versions.len()+1),"latex":"\\documentclass{article}\\begin{document}A bounded argument.\\end{document}","bibliography":"","evidenceIds":[],"limitations":["Source coverage"],"responseToReview":[],"files":[]})
        }
        Phase::Review { .. } => {
            json!({"summary":"The central claim is appropriately bounded","findings":[],"strengths":["Explicit source criticism"],"limitations":["Coverage"]})
        }
        Phase::AssessPaper { .. } => {
            json!({"summary":"A coherent bounded paper draft","sound":true,"contribution":"Useful interpretation","support":"Argument supported within the provided record","originality":"Uncertain beyond the inspected literature","completeness":"Complete bounded draft","remainingWork":["Broader archive access"]})
        }
        Phase::Rank => {
            json!({"papers":p.papers.iter().enumerate().filter(|(_,p)|p.assessment.is_some()).map(|(i,p)|json!({"candidateId":p.candidate_id,"rank":i+1,"reason":"Stronger interpretive contribution","uncertainty":"A close comparison"})).collect::<Vec<_>>()})
        }
        Phase::Deliver => json!({"delivered":true}),
        _ => panic!("No scripted response for {:?}", p.run.phase),
    }
}
