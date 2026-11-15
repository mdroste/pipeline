use super::*;

#[test]
fn assumption_branch_is_open_and_does_not_change_the_original_argument() {
    let f = fixture(false);
    let note = project::TheoryNote {
        kind: "proposition".into(),
        title: "Original proposition".into(),
        statement: "An equilibrium exists.".into(),
        body: "Argument under the stated assumptions.".into(),
        assumptions: vec!["Compact feasible set".into()],
        assumption_ids: vec![],
        anchor_ids: vec![],
        related_ids: vec![],
        unresolved_steps: vec![],
        status: "open".into(),
        rejection_reason: String::new(),
        origin: "manual".into(),
        promotions: vec![],
        imported_from: None,
        imported_status: None,
    };
    let made = project::studio_mutate(
        &f.store,
        project::StudioMutation {
            workspace_id: f.ws.clone(),
            operation_id: "original".into(),
            action: project::StudioAction::SaveTheory {
                id: None,
                expected_revision: 0,
                note,
            },
        },
    )
    .unwrap();
    let id = made["id"].as_str().unwrap();
    let source = ResearchObjectRef {
        kind: "record".into(),
        id: id.into(),
        revision: "1".into(),
        start: None,
        end: None,
    };
    let branch = theory::branch(
        &f.store,
        theory::Branch {
            workspace_id: f.ws.clone(),
            source,
            title: "Remove compactness".into(),
            assumptions: vec!["Closed but unbounded feasible set".into()],
            question: "Does existence survive without compactness?".into(),
            operation_id: "branch".into(),
        },
    )
    .unwrap();
    assert_eq!(branch.body["note"]["body"]["status"], "open");
    assert_eq!(
        project::record(&f.store, &f.ws, id, "theory").unwrap().body["assumptions"],
        json!(["Compact feasible set"])
    );
    assert_eq!(
        project::records(&f.store, &f.ws, "theory").unwrap().len(),
        2
    );
}
#[test]
fn campaigns_keep_two_rounds_and_flag_unsubstantiated_analysis_claims() {
    let f = fixture(true);
    fs::write(f.root.join("report.md"), "Comment one: test robustness.\n").unwrap();
    let paper = research::import_paper(
        &f.store,
        research::ImportPaperRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            title: "Round one manuscript".into(),
            role: "manuscript".into(),
            path: f.root.join("report.md").to_string_lossy().into_owned(),
            operation_id: "paper".into(),
        },
    )
    .unwrap();
    let revision = paper.revision.unwrap();
    let responses = project::studio_mutate(
        &f.store,
        project::StudioMutation {
            workspace_id: f.ws.clone(),
            operation_id: "report".into(),
            action: project::StudioAction::ImportReport {
                revision_id: revision.id.clone(),
                comments: vec![project::ReportComment {
                    number: "2".into(),
                    start: 0,
                    end: "Comment one: test robustness.".len(),
                    text: "Comment one: test robustness.".into(),
                }],
            },
        },
    )
    .unwrap();
    let response = responses[0]["id"].as_str().unwrap();
    let d = project::ResponseDecision {
        number: "2".into(),
        category: "missing_robustness".into(),
        severity: "high".into(),
        disposition: "addressed".into(),
        draft: "We added a robustness analysis.".into(),
        reports_analysis_added: true,
        ..Default::default()
    };
    project::studio_mutate(
        &f.store,
        project::StudioMutation {
            workspace_id: f.ws.clone(),
            operation_id: "response".into(),
            action: project::StudioAction::SaveResponse {
                id: response.into(),
                expected_revision: 1,
                response: d,
            },
        },
    )
    .unwrap();
    let mut c = campaigns::Campaign {
        round: 1,
        manuscript: ResearchObjectRef {
            kind: "paper".into(),
            id: revision.id,
            revision: revision.content_hash,
            start: None,
            end: None,
        },
        comments: vec![campaigns::Comment {
            response: ResearchObjectRef {
                kind: "record".into(),
                id: response.into(),
                revision: "2".into(),
                start: None,
                end: None,
            },
            required_outputs: vec![],
            researcher_addressed: true,
            judgment: "Researcher assertion, pending linked evidence".into(),
        }],
        change_summary: "Claimed robustness addition".into(),
        review_scope: "Selected response and unchanged manuscript".into(),
        broad_change: false,
        previous_round: None,
    };
    let first = campaigns::save(&f.store, &f.ws, "First round", c.clone(), None, "first").unwrap();
    let status = campaigns::completion(&f.store, &f.ws, &first.id).unwrap();
    assert_eq!(status["comments"][0]["researcherAddressed"], true);
    assert_eq!(status["comments"][0]["linkChecksPass"], false);
    assert!(status["comments"][0]["flags"].as_array().unwrap().len() >= 3);
    c.round = 2;
    c.previous_round = Some(first.id.clone());
    let second = campaigns::save(&f.store, &f.ws, "Second round", c, None, "second").unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(
        desk::record(&f.store, &f.ws, &first.id).unwrap().body["round"],
        1
    );
}
