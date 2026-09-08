use super::model::*;
use std::fmt::Write;

pub fn render(m: &Mission) -> String {
    let mut s = format!(
        "# {}\n\n{}\n\nStatus: {}. {}\n\n",
        m.definition.name, m.definition.objective, m.state, m.reason
    );
    let _=writeln!(s,"## Resources\n\n{} of {} managed actions; {} of {} active execution seconds; {} of {} investigation rounds. {} interruptions requiring attention.\n",m.actions_reserved,m.definition.budget.max_actions,m.active_seconds,m.definition.budget.active_seconds,m.rounds.len(),m.definition.budget.max_rounds,m.attention_count);
    s.push_str("## Completion criteria\n\n");
    let last = m.rounds.iter().rev().find_map(|r| r.challenge.as_ref());
    for (i, criterion) in m.definition.criteria.iter().enumerate() {
        let assessment = last.and_then(|c| c.criteria.iter().find(|a| a.criterion == i));
        let state = assessment
            .map(|a| match a.status {
                CriterionStatus::Met => "assessed met",
                CriterionStatus::Unmet => "unmet",
                CriterionStatus::Unknown => "unknown",
            })
            .unwrap_or("not assessed");
        let _ = writeln!(s, "{}. {} — {}", i + 1, criterion, state);
        if let Some(a) = assessment {
            let _ = writeln!(
                s,
                "   {} Evidence: {}.\n",
                a.explanation,
                a.evidence_ids.join(", ")
            );
        }
    }
    s.push_str("\n## Agenda\n\n");
    for g in &m.goals {
        let _ = writeln!(
            s,
            "- **{}** ({}, {}): {}\n  Resolving evidence: {}. {}",
            g.spec.id,
            g.state,
            g.spec.question,
            g.spec.rationale,
            g.spec.resolving_evidence,
            g.assessment
        );
    }
    s.push_str("\n## Investigations\n\n");
    for r in &m.rounds {
        let _ = writeln!(
            s,
            "### Round {}\n\n{}\n\nSelection: {}\n",
            r.number, r.plan.summary, r.plan.selection_reason
        );
        if let Some(c) = &r.selected {
            let _ = writeln!(
                s,
                "Question: {}\n\nUncertainty: {}\n\nPossible outcomes: {}\n",
                c.question,
                c.uncertainty,
                c.possible_outcomes.join("; ")
            );
        }
        if let Some(f) = &r.finding {
            let _ = writeln!(
                s,
                "{}\n\nOutcome: {}\n\nMethod: {}. Tested domain: {}.\n",
                f.summary, f.outcome, f.method, f.tested_domain
            );
            for l in &f.limitations {
                let _ = writeln!(s, "- Limitation: {l}");
            }
        }
        if let Some(c) = &r.challenge {
            let _ = writeln!(
                s,
                "\nChallenge: {}\n\nTested domain: {}\n",
                c.summary, c.tested_domain
            );
            for l in c.limitations.iter().chain(&c.unresolved) {
                let _ = writeln!(s, "- {l}");
            }
        }
        if !r.evidence.is_empty() {
            s.push_str("\nRetained evidence:\n\n");
            for (id, v) in &r.evidence {
                let _ = writeln!(s, "- `{id}`: {}", evidence_description(v));
            }
        }
        let _ = writeln!(s, "\nChild task receipts: {}\n", r.child_ids.join(", "));
    }
    if !m.questions.is_empty() {
        s.push_str("## Researcher decisions\n\n");
        for q in &m.questions {
            let _ = writeln!(
                s,
                "- {}\n  Why: {}\n  {}\n",
                q.spec.question,
                q.spec.why_needed,
                q.answer.as_deref().unwrap_or("Awaiting your answer")
            );
        }
    }
    if !m.methods.is_empty() {
        s.push_str("## Reusable method proposals\n\n");
        for method in &m.methods {
            let _ = writeln!(
                s,
                "### {}\n\nUse when: {}\n\n{}\n\nLimitations: {}\n\nEvidence: {}. {}\n",
                method.spec.name,
                method.spec.when_to_use,
                method.spec.procedure,
                method.spec.limitations,
                method.spec.evidence_ids.join(", "),
                if method.retained {
                    "Retained for explicit reuse."
                } else {
                    "Unreviewed proposal."
                }
            );
        }
    }
    s.push_str("\nModel assessments remain separate from researcher acceptance. Numerical checks establish results only over their tested domain. Native and host execution retain their recorded permission and dependency-coverage limits.\n");
    s
}

fn evidence_description(value: &serde_json::Value) -> String {
    let kind = value["kind"].as_str().unwrap_or("record");
    let label = value["filename"]
        .as_str()
        .or_else(|| value["label"].as_str())
        .or_else(|| value["title"].as_str())
        .unwrap_or("Retained record");
    let hash = value["hash"].as_str().unwrap_or("");
    format!(
        "{label} ({kind}). {}",
        if hash.is_empty() {
            "Inspect in Findings or the complete JSON export.".into()
        } else {
            format!("SHA-256: {hash}.")
        }
    )
}
