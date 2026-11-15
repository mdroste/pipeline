//! A scientific assessment is about an exact subject (round or manuscript version).
//! The assessor supplies its inspection and objections; the host supplies the
//! assessor's child identity, the provenance of each retained item, and the
//! coverage decision. A positive verdict alone cannot satisfy a requirement.
//! Researcher decisions are written only by explicit UI commands.
use super::store::{hash, now, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Theory,
    Empirical,
    Interpretation,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Assumptions,
    Derivation,
    Data,
    Procedure,
    Output,
    Table,
    Passage,
    Review,
}
impl Role {
    fn label(self) -> &'static str {
        match self {
            Self::Assumptions => "assumptions",
            Self::Derivation => "derivation or counterexample",
            Self::Data => "data version",
            Self::Procedure => "code or approved plan",
            Self::Output => "checked output",
            Self::Table => "table or reported result location",
            Self::Passage => "source version and passage",
            Self::Review => "referee assessment",
        }
    }
}
impl Kind {
    fn roles(self) -> &'static [Role] {
        use Role::*;
        match self {
            Self::Theory => &[Assumptions, Derivation],
            Self::Empirical => &[Data, Procedure, Output, Table],
            Self::Interpretation => &[Passage],
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    Supported,
    Refuted,
    Unresolved,
    Infeasible,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Inspection {
    pub role: Role,
    pub evidence_id: String,
    pub location: String,
    pub observed: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Comparison {
    pub reported: f64,
    pub reproduced: f64,
    pub decimal_places: u8,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Submission {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,
    pub subject: usize,
    pub claim: String,
    pub kind: Kind,
    pub scope: String,
    pub verdict: Verdict,
    pub inspections: Vec<Inspection>,
    #[serde(default)]
    pub comparison: Option<Comparison>,
    pub objections: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    InvestigatorMemo,
    Manuscript,
    CapturedFile,
    SuppliedInput,
    ProjectAccount,
    RefereeAssessment,
    SourceRevision,
    CheckedExecution,
    FailedExecution,
    Other,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    #[serde(flatten)]
    pub inspection: Inspection,
    pub origin: Origin,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Claim {
    pub goal_id: Option<String>,
    pub subject: usize,
    pub target: String,
    pub claim: String,
    pub scope: String,
    pub kind: Option<Kind>,
    pub verdict: Verdict,
    pub required: Vec<Role>,
    pub evidence: Vec<Evidence>,
    pub comparison: Option<Comparison>,
    pub objections: Vec<String>,
    pub meets_requirements: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub schema_version: u32,
    pub subject_version: String,
    pub assessor_role: String,
    pub assessor_child: String,
    pub claims: Vec<Claim>,
    pub objections: Vec<String>,
    pub supported: bool,
    pub accepted_at: Option<i64>,
}
impl Record {
    pub fn accept(&mut self, version: &str) -> Result<()> {
        if self.schema_version != 1 || self.subject_version != version || !self.supported {
            return Err(
                "Only the supported assessment of this exact version can be accepted".into(),
            );
        }
        self.accepted_at.get_or_insert_with(now);
        Ok(())
    }
    pub fn describe(&self) -> String {
        let mut s = format!(
            "Assessment: {}. Researcher acceptance: {}. Subject: {}. Assessor: {} (action {}).\n",
            if self.supported {
                "evidence requirements met"
            } else {
                "unresolved"
            },
            if self.accepted_at.is_some() {
                "accepted"
            } else {
                "not accepted"
            },
            self.subject_version,
            self.assessor_role,
            self.assessor_child
        );
        for c in &self.claims {
            s.push_str(&format!(
                "- {} — {}. Scope: {}. Assessment: {:?}.\n",
                c.target, c.claim, c.scope, c.verdict
            ));
            for e in &c.evidence {
                s.push_str(&format!(
                    "  {}: {} at {} — {}.\n",
                    e.inspection.role.label(),
                    e.inspection.evidence_id,
                    e.inspection.location,
                    e.inspection.observed
                ));
            }
            if let Some(v) = &c.comparison {
                s.push_str(&format!(
                    "  Reported {}; reproduced {} ({} decimal places).\n",
                    v.reported, v.reproduced, v.decimal_places
                ));
            }
            for issue in &c.objections {
                s.push_str(&format!("  Unresolved: {issue}\n"));
            }
        }
        for issue in &self.objections {
            s.push_str(&format!("Unresolved: {issue}\n"));
        }
        s
    }
}
fn bounded(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.contains('\0')
}
fn origin(id: &str, v: &Value) -> Origin {
    match v["kind"].as_str() {
        Some("refereeAssessment") => Origin::RefereeAssessment,
        Some("executionReceipt")
            if v["result"]["passed"] == true
                && v["result"]["receipt"]["outcome"] == "completed" =>
        {
            Origin::CheckedExecution
        }
        Some("executionReceipt") => Origin::FailedExecution,
        Some("researchSource")
            if v["reference"]["revision"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
                && v["object"]["text"].as_str().is_some_and(|s| !s.is_empty()) =>
        {
            if matches!(v["reference"]["kind"].as_str(), Some("source" | "paper"))
                && matches!(
                    v["object"]["access"].as_str(),
                    Some("full" | "abstract" | "partial")
                )
            {
                Origin::SourceRevision
            } else {
                Origin::ProjectAccount
            }
        }
        Some("artifact") if v["hash"].as_str().is_some_and(|s| !s.is_empty()) => {
            if id.ends_with("-memo") || id.ends_with("-dossier") {
                Origin::InvestigatorMemo
            } else if id.starts_with("input:") {
                Origin::SuppliedInput
            } else if matches!(id, "paper.md" | "paper.tex" | "references.bib") {
                Origin::Manuscript
            } else {
                Origin::CapturedFile
            }
        }
        _ => Origin::Other,
    }
}
fn usable(role: Role, o: Origin) -> bool {
    use Origin::*;
    match role {
        Role::Assumptions | Role::Derivation => matches!(
            o,
            InvestigatorMemo | Manuscript | CapturedFile | SourceRevision | SuppliedInput
        ),
        Role::Data | Role::Procedure => {
            matches!(
                o,
                CapturedFile | SuppliedInput | SourceRevision | CheckedExecution
            )
        }
        Role::Output => o == CheckedExecution,
        Role::Table => matches!(o, Manuscript | CapturedFile | InvestigatorMemo),
        Role::Passage => matches!(o, SourceRevision | SuppliedInput),
        Role::Review => o == RefereeAssessment,
    }
}

// A completed check on other data or code cannot reproduce this table. The
// receipt can itself identify its immutable inputs and approved command.
fn binds_input(receipt_id: &str, receipt: &Value, item: &Evidence) -> bool {
    let files = receipt["inputManifest"]["files"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let usable_manifest = receipt["inputManifest"]["complete"] != false;
    if item.inspection.evidence_id == receipt_id {
        return match item.inspection.role {
            Role::Data => usable_manifest && files.iter().any(|v| v["contentHash"].is_string()),
            Role::Procedure => {
                receipt["dependencyHash"].is_string()
                    && receipt["command"].as_array().is_some_and(|c| !c.is_empty())
            }
            _ => false,
        };
    }
    usable_manifest && files.iter().any(|v| v["contentHash"] == item.digest)
}
// Read a number from the retained result, never from the assessor's prose.
fn output_at(receipt: &Value, site: &str) -> Option<f64> {
    let n = if let Some(path) = site.strip_prefix("outputManifest.") {
        path.split('.')
            .fold(&receipt["outputManifest"], |v, k| &v[k])
            .as_f64()?
    } else if let Some(id) = site.strip_prefix("result:") {
        let results = receipt["structuredResults"].as_array()?;
        let mut matching = results.iter().filter(|v| v["id"] == id);
        let value = matching.next()?["estimate"].as_f64()?;
        if matching.next().is_some() {
            return None;
        }
        value
    } else {
        let line = site
            .strip_prefix("stdout:")?
            .parse::<usize>()
            .ok()?
            .checked_sub(1)?;
        let text = receipt["stdout"].as_str()?.lines().nth(line)?;
        text.rsplit(['=', ':']).next()?.trim().parse::<f64>().ok()?
    };
    n.is_finite().then_some(n)
}
fn check_reproduction(
    input: &Submission,
    evidence: &[Evidence],
    retained: &BTreeMap<String, Value>,
    issues: &mut Vec<String>,
) {
    let Some(c) = input
        .comparison
        .as_ref()
        .filter(|v| v.reported.is_finite() && v.reproduced.is_finite() && v.decimal_places <= 12)
    else {
        issues.push(
            "Missing comparison of the reported and reproduced values at the table’s precision."
                .into(),
        );
        return;
    };
    let tolerance = 0.5 * 10f64.powi(-i32::from(c.decimal_places));
    if (c.reported - c.reproduced).abs() > tolerance {
        issues.push(format!(
            "Reported value {} differs from reproduced value {} at the stated precision.",
            c.reported, c.reproduced
        ));
    }
    let mut connected = false;
    for out in evidence
        .iter()
        .filter(|e| e.inspection.role == Role::Output && e.origin == Origin::CheckedExecution)
    {
        let id = &out.inspection.evidence_id;
        let receipt = &retained[id]["result"]["receipt"];
        let site = out.inspection.location.trim();
        match output_at(receipt, site) {
            Some(n) if (n - c.reproduced).abs() <= tolerance => {}
            Some(_) => issues
                .push("The reproduced value disagrees with the cited execution output.".into()),
            None => {
                issues.push("The cited execution location has no readable numerical result.".into())
            }
        }
        connected |= [Role::Data, Role::Procedure].iter().all(|role| {
            evidence.iter().any(|e| {
                e.inspection.role == *role && usable(*role, e.origin) && binds_input(id, receipt, e)
            })
        });
    }
    if !connected {
        issues.push(
            "No completed check links the inspected data and procedure to this output.".into(),
        );
    }
}

/// Missing inspections become named unresolved obligations; fabricated
/// references and malformed responses are rejected rather than recorded as fact.
pub fn assess(
    subject_version: String,
    assessor: (&str, &str),
    targets: &[String],
    input: &[Submission],
    retained: &BTreeMap<String, Value>,
    objections: Vec<String>,
    positive: bool,
) -> Result<Record> {
    if serde_json::to_vec(input).map_err(super::store::err)?.len() > 64 * 1024
        || input.len() > 48
        || targets.is_empty()
        || targets.len() > 64
    {
        return Err("Invalid claim inventory".into());
    }
    let mut claims = Vec::new();
    for a in input {
        let target = targets
            .get(a.subject)
            .ok_or("Claim names an unknown criterion or manuscript claim")?;
        if !bounded(&a.claim, 2000)
            || !bounded(&a.scope, 2000)
            || a.inspections.len() > 12
            || a.objections.len() > 12
            || a.objections.iter().any(|s| !bounded(s, 2000))
        {
            return Err("Invalid claim, inspected scope or objections".into());
        }
        let mut issues = a.objections.clone();
        let mut evidence = vec![];
        let mut covered = BTreeSet::new();
        let mut sites = BTreeSet::new();
        for i in &a.inspections {
            if !bounded(&i.location, 500) || !bounded(&i.observed, 2000) {
                return Err("Each inspection needs a location and an observation".into());
            }
            let v = retained
                .get(&i.evidence_id)
                .ok_or("Claim cites evidence outside this assessment")?;
            let o = origin(&i.evidence_id, v);
            let digest = match v["hash"].as_str() {
                Some(h) => h.to_string(),
                None => hash(v)?,
            };
            if usable(i.role, o)
                && sites.insert((digest.clone(), i.location.trim(), i.observed.trim()))
            {
                covered.insert(i.role);
            }
            evidence.push(Evidence {
                inspection: i.clone(),
                origin: o,
                digest,
            });
        }
        for role in a.kind.roles() {
            if !covered.contains(role) {
                issues.push(format!("Missing inspection: {}.", role.label()));
            }
        }
        if a.kind == Kind::Empirical {
            check_reproduction(a, &evidence, retained, &mut issues);
        }
        let meets = issues.is_empty() && matches!(a.verdict, Verdict::Supported | Verdict::Refuted);
        claims.push(Claim {
            goal_id: a.goal_id.clone(),
            subject: a.subject,
            target: target.clone(),
            claim: a.claim.clone(),
            scope: a.scope.clone(),
            kind: Some(a.kind),
            verdict: a.verdict,
            required: a.kind.roles().to_vec(),
            evidence,
            comparison: a.comparison.clone(),
            objections: issues,
            meets_requirements: meets,
        });
    }
    for (subject, target) in targets.iter().enumerate() {
        if !claims.iter().any(|c| c.subject == subject) {
            claims.push(Claim {
                subject,
                goal_id: None,
                target: target.clone(),
                claim: target.clone(),
                scope: "Not inspected".into(),
                kind: None,
                verdict: Verdict::Unresolved,
                required: vec![],
                evidence: vec![],
                comparison: None,
                objections: vec!["No claim inspection was supplied for this subject.".into()],
                meets_requirements: false,
            });
        }
    }
    let supported =
        positive && objections.is_empty() && claims.iter().all(|c| c.meets_requirements);
    Ok(Record {
        schema_version: 1,
        subject_version,
        assessor_role: assessor.0.into(),
        assessor_child: assessor.1.into(),
        claims,
        objections,
        supported,
        accepted_at: None,
    })
}
pub fn infeasible(
    subject_version: String,
    assessor: (&str, &str),
    target: String,
    scope: String,
    reason: String,
) -> Record {
    Record {
        schema_version: 1,
        subject_version,
        assessor_role: assessor.0.into(),
        assessor_child: assessor.1.into(),
        claims: vec![Claim {
            goal_id: None,
            subject: 0,
            target: target.clone(),
            claim: target,
            scope,
            kind: None,
            verdict: Verdict::Infeasible,
            required: vec![],
            evidence: vec![],
            comparison: None,
            objections: vec![reason],
            meets_requirements: false,
        }],
        objections: vec![],
        supported: false,
        accepted_at: None,
    }
}
pub fn example() -> Value {
    json!([{"subject":0,"claim":"The central statement being tested","kind":"theory","scope":"Population, assumptions or sources actually examined","verdict":"unresolved","inspections":[{"role":"assumptions","evidenceId":"exact retained ID","location":"section, line, passage or table cell","observed":"What you independently checked at that location"}],"comparison":null,"objections":["Unresolved proof obligation"]}])
}
pub const INSTRUCTIONS:&str="Assess each zero-based subject in claimSubjects. Record the actual claim and scope, verdict (supported, refuted, unresolved or infeasible), inspections and open objections. For theory inspect assumptions and derivation (a self-contained counterexample can suffice); for empirical inspect data, procedure, output (completed retained execution) and table; for interpretation inspect a passage at its exact source version. For an empirical numerical result supply comparison {reported, reproduced, decimalPlaces} at the reported precision. For output, location must be result:<structured result ID>, outputManifest.<numeric field>, or stdout:<one-based line containing a number or label=number>. Use these literal role names. Cite only listed evidence IDs; identify locations and what you independently found, not an author's assurance. Referee assessments (role review) can document objections but do not replace the inspected primary evidence. Missing evidence stays unresolved. A supported null or refutation is a resolving result.";
#[cfg(test)]
pub(crate) mod benchmark;
#[cfg(test)]
mod tests;
