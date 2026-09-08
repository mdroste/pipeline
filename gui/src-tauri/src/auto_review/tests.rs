use super::*;

fn configured_auto_review() -> PipelineConfig {
    let mut config = PipelineConfig {
        steps: steps(),
        merge: Default::default(),
        outputs: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: orientation_prompt(),
        orientation_schema: Some(orientation_schema()),
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };
    for step in &mut config.steps {
        step.context.include = vec![ArtifactSelector::Survey];
    }
    let synthesis = config
        .steps
        .iter_mut()
        .find(|step| step.id == "auto_synthesis")
        .unwrap();
    synthesis.context.include.extend(
        ["auto_contribution", "auto_consistency", "auto_exposition"]
            .into_iter()
            .map(|step| ArtifactSelector::Step {
                step: step.to_string(),
                parts: vec![StepArtifactPart::Report],
                glob: String::new(),
            }),
    );
    config
}

fn synthesis_report_inputs(config: &PipelineConfig) -> Vec<&str> {
    config
        .steps
        .iter()
        .find(|step| step.id == "auto_synthesis")
        .unwrap()
        .context
        .include
        .iter()
        .filter_map(|selector| match selector {
            ArtifactSelector::Step { step, parts, .. } if parts == &[StepArtifactPart::Report] => {
                Some(step.as_str())
            }
            _ => None,
        })
        .collect()
}

fn valid_orientation() -> serde_json::Value {
    serde_json::json!({
        "metadata": {
            "title": "A paper", "authors": [], "date": "", "paper_type": "theory",
            "page_count": 0,
            "has_appendix": true, "has_online_appendix": false
        },
        "review_plan": {
            "primary_domain": "mathematics",
            "subject": "partial differential equations",
            "paper_forms": ["formal_theory"],
            "methods": ["proof"],
            "subject_specialist_ids": ["subject_mathematics_pde"],
            "method_specialist_ids": ["formal_proofs"],
            "genre": "research_article",
            "selection_notes": [
                {"id": "subject_mathematics_pde", "reason": "The main result concerns a nonlinear PDE."},
                {"id": "formal_proofs", "reason": "The theorem and proof carry the contribution."}
            ],
            "routing_uncertainty": []
        },
        "sections": [], "formal_results": [], "tables_figures": [],
        "notation": [], "stated_contribution": "A contribution.",
        "key_references": [], "extraction_quality_notes": []
    })
}

#[test]
fn catalog_ids_prompts_and_fallbacks_are_complete() {
    assert!(!SUBJECTS.is_empty());
    assert!(!METHODS.is_empty());
    let mut ids = std::collections::HashSet::new();
    let mut disciplines = std::collections::HashMap::<&str, bool>::new();
    for specialist in SUBJECTS {
        assert!(ids.insert(specialist.id), "duplicate id {}", specialist.id);
        assert!(specialist.id.starts_with("subject_"));
        let prompt = subject_prompt(specialist);
        assert_specialist_prompt_contract(&prompt, specialist.label);
        let fallback = disciplines.entry(specialist.discipline_id).or_default();
        *fallback |= specialist.level == SubjectLevel::Discipline;
    }
    let mut seen_families = std::collections::HashSet::new();
    let mut current_family = "";
    for specialist in METHODS {
        assert!(ids.insert(specialist.id), "duplicate id {}", specialist.id);
        assert!(specialist.prompt.len() > 200);
        let prompt = method_prompt(specialist);
        assert_specialist_prompt_contract(&prompt, specialist.label);
        // Families must be contiguous so grouped rendering stays faithful.
        if specialist.family_id != current_family {
            assert!(
                seen_families.insert(specialist.family_id),
                "family {} is not contiguous",
                specialist.family_id
            );
            current_family = specialist.family_id;
        }
        assert!(!specialist.family_label.is_empty(), "{}", specialist.id);
    }
    for genre in GENRES {
        assert!(ids.insert(genre.id), "duplicate id {}", genre.id);
        assert!(genre.id.starts_with("genre_"), "{}", genre.id);
        assert_ne!(genre.id, RESEARCH_ARTICLE_GENRE);
        // Genre context is a compact shared paragraph, not a referee
        // prompt; it must never carry its own report-output contract.
        assert!(genre.prompt.len() > 200, "{}", genre.id);
        assert!(!genre.prompt.contains("## Output"), "{}", genre.id);
    }
    assert!(!disciplines.is_empty());
    assert!(disciplines.values().all(|fallback| *fallback));
    // Every family has at most one broad fallback role.
    for family in &seen_families {
        assert!(
            METHODS
                .iter()
                .filter(|specialist| specialist.family_id == *family
                    && specialist.level == MethodLevel::Family)
                .count()
                <= 1,
            "family {family} has more than one fallback"
        );
    }

    let view = catalog();
    assert_eq!(view.subject_count, SUBJECTS.len());
    assert_eq!(view.method_count, METHODS.len());
    assert_eq!(view.disciplines.len(), disciplines.len());
    assert_eq!(view.method_families.len(), seen_families.len());
    assert_eq!(
        view.disciplines
            .iter()
            .map(|discipline| discipline.roles.len())
            .sum::<usize>(),
        SUBJECTS.len()
    );
    assert_eq!(
        view.method_families
            .iter()
            .map(|family| family.roles.len())
            .sum::<usize>(),
        METHODS.len()
    );
}

#[test]
fn copyable_specialist_steps_are_owned_snapshots() {
    let id = "subject_economics_macro";
    let mut copied = copyable_specialist_step(id).unwrap();
    let default_prompt = copied.prompt.clone();

    copied.label = "My macro reviewer".to_string();
    copied.prompt.push_str("\n\nWorkflow-local addition.");

    let fresh = copyable_specialist_step(id).unwrap();
    assert_eq!(fresh.id, id);
    assert_eq!(fresh.label, "Economics — Macroeconomics (General)");
    assert_eq!(fresh.prompt, default_prompt);
    assert_ne!(fresh.prompt, copied.prompt);
    assert!(copyable_specialist_step("not_in_the_catalog").is_none());
}

fn assert_specialist_prompt_contract(prompt: &str, label: &str) {
    assert!(prompt.starts_with(&format!("# {label}\n\n")), "{label}");
    assert_eq!(
        prompt.lines().filter(|line| line.starts_with("# ")).count(),
        1,
        "{label} has more than one top-level heading"
    );
    assert_report_output_fields(prompt, label);
    assert!(
        prompt.contains("Do not add operational instructions for harmful activity"),
        "{label} is missing the safety boundary"
    );
}

fn assert_report_output_fields(prompt: &str, label: &str) {
    for required in [
        "## Output",
        "Populate the supplied findings schema",
        "return an empty findings array",
        "invent a locator",
    ] {
        assert!(prompt.contains(required), "{label} is missing {required}");
    }
    assert!(
        !prompt.contains("- `title`:"),
        "{label} duplicates the structured schema field descriptions"
    );
    assert!(
        !prompt.contains("Severity"),
        "{label} still asks for a severity label"
    );
}

#[test]
fn universal_reviewers_use_the_report_viewer_contract() {
    for (label, prompt) in [
        ("Contribution & Literature", CORE_CONTRIBUTION),
        ("Claims & Consistency", CORE_CONSISTENCY),
        ("Exposition & Architecture", CORE_EXPOSITION),
    ] {
        assert!(prompt.starts_with("# "), "{label}");
        assert_report_output_fields(prompt, label);
    }
    assert!(SYNTHESIS.contains("Populate the supplied findings schema"));
    assert!(SYNTHESIS.contains("Retain up to forty findings"));
    assert!(SYNTHESIS.contains("never use an ordinal"));
    assert!(VALIDATE.contains("{last_output}"));
    assert!(VALIDATE.contains("retain its exact input `id`"));
    assert!(VALIDATE.contains("do not add new findings"));
    assert!(SYNTHESIS.contains("`reviewer_ids`"));
    assert!(VALIDATE.contains("reviewer and call lineage"));
}

#[test]
fn shared_prompt_layers_stay_compact() {
    let words = |prompt: &str| prompt.split_whitespace().count();
    assert!(words(ORIENTATION_TEMPLATE) <= 750);
    assert!(words(SUBJECT_REVIEW_BASE) <= 300);
    assert!(words(METHOD_REVIEW_BASE) <= 250);
    assert!(words(CORE_CONTRIBUTION) <= 275);
    assert!(words(CORE_CONSISTENCY) <= 225);
    assert!(words(CORE_EXPOSITION) <= 200);
    assert!(words(VALIDATE) <= 400);
}

#[test]
fn generated_prompt_and_schema_cover_the_catalog() {
    let template = orientation_prompt();
    assert!(template.contains("{subject_catalog}"));
    assert!(template.contains("{method_catalog}"));
    assert!(template.contains("{genre_catalog}"));
    let prompt = expand_orientation_prompt(&template);
    assert!(!prompt.contains("{subject_catalog}"));
    assert!(!prompt.contains("{method_catalog}"));
    assert!(!prompt.contains("{genre_catalog}"));
    for specialist in SUBJECTS {
        assert!(prompt.contains(&format!("`{}`", specialist.id)));
    }
    for specialist in METHODS {
        assert!(prompt.contains(&format!("`{}`", specialist.id)));
        assert!(prompt.contains(&format!("### {}", specialist.family_label)));
    }
    for specialist in GENRES {
        assert!(prompt.contains(&format!("`{}`", specialist.id)));
    }
    let compact = orientation_schema();
    crate::pipeline::structured::validate_schema(&compact).unwrap();
    assert_eq!(
        compact.pointer(
            "/properties/review_plan/properties/subject_specialist_ids/items/x-pipeline-catalog"
        ),
        Some(&serde_json::json!(SUBJECT_CATALOG))
    );
    assert!(compact
        .pointer("/properties/review_plan/properties/subject_specialist_ids/items/enum")
        .is_none());

    let resolved = resolve_schema_catalogs(&compact).unwrap();
    let subject_ids = resolved
        .pointer("/properties/review_plan/properties/subject_specialist_ids/items/enum")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    let method_ids = resolved
        .pointer("/properties/review_plan/properties/method_specialist_ids/items/enum")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert_eq!(subject_ids.len(), SUBJECTS.len());
    assert_eq!(method_ids.len(), METHODS.len());
    assert!(resolved.to_string().find(CATALOG_REFERENCE_KEY).is_none());
    crate::pipeline::structured::validate_schema(&resolved).unwrap();
    let provider = crate::pipeline::structured::provider_schema(&resolved).unwrap();
    let provider_bytes = serde_json::to_vec(&provider).unwrap().len();
    assert!(
        provider_bytes + 1024 <= crate::pipeline::structured::MAX_PROVIDER_SCHEMA_BYTES,
        "resolved Auto Review schema needs at least 1 KiB of catalog growth headroom; got {provider_bytes} bytes"
    );
    resolve_schema_catalogs(&quick_orientation_schema()).unwrap();
    assert!(resolve_schema_catalogs(&serde_json::json!({
        "type": "string",
        CATALOG_REFERENCE_KEY: "missing.catalog"
    }))
    .unwrap_err()
    .contains("unknown Pipeline catalog reference"));
}

#[test]
fn auto_review_marker_requires_the_complete_host_contract() {
    let mut schema = orientation_schema();
    schema["properties"]["review_plan"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("method_specialist_ids");
    assert!(validate_schema_settings(&schema)
        .unwrap_err()
        .contains("method_specialist_ids"));

    let mut custom = schema;
    custom
        .as_object_mut()
        .unwrap()
        .remove("x-pipeline-contract");
    assert!(validate_schema_settings(&custom).is_ok());

    let typo = serde_json::json!({
        "type": "object",
        "x-pipeline-contract": "auto-reveiw-v1"
    });
    assert!(validate_schema_settings(&typo)
        .unwrap_err()
        .contains("Unsupported x-pipeline-contract"));

    assert!(validate_profile_contract_identity(&steps(), None)
        .unwrap_err()
        .contains("require x-pipeline-contract"));
}

#[test]
fn specialist_artifacts_round_trip_to_the_referee_format() {
    let artifact = serde_json::json!({"findings": [{
        "title": "Sign error in Proposition 2",
        "in_the_paper": "Claim",
        "problem": "Analysis",
        "consequence": "Effect",
        "what_would_help": "Fix",
        "evidence": [
            {"evidence_type": "document", "verification_status": "unverified", "page": 12, "description": "Proposition 2", "quote": "the sign flips"},
            {"evidence_type": "source", "verification_status": "unverified", "source_path": "model.tex", "line_start": 4, "line_end": 9, "description": "model source"}
        ]
    }]});
    crate::pipeline::structured::validate(&specialist_schema(), &artifact).unwrap();
    let markdown = specialist_report_markdown(&artifact).unwrap();
    assert!(markdown.starts_with("**#1. Sign error in Proposition 2**"));
    for field in [
        "- **In the paper:** Claim",
        "- **The problem:** Analysis",
        "- **Consequence:** Effect",
        "- **What would help:** Fix",
    ] {
        assert!(markdown.contains(field), "missing {field}");
    }
    assert!(markdown.contains("p. 12 · Proposition 2 · “the sign flips”"));
    assert!(markdown.contains("model.tex:4–9"));

    // Empty findings render the human sentinel; the model never emits it.
    assert_eq!(
        specialist_report_markdown(&serde_json::json!({"findings": []})).unwrap(),
        "No material issues identified."
    );
    // Canonical consolidated findings are not specialist-shaped.
    assert!(specialist_report_markdown(
        &serde_json::json!({"findings": [{"id": "a", "title": "T", "body": "B"}]})
    )
    .is_none());
    // Schema rejects a finding with no evidence locator at all.
    assert!(crate::pipeline::structured::validate(
        &specialist_schema(),
        &serde_json::json!({"findings": [{
            "title": "T", "in_the_paper": "P", "problem": "Q",
            "consequence": "C", "what_would_help": "H", "evidence": []
        }]})
    )
    .is_err());
}

#[test]
fn quick_schema_limits_methods_and_total_agents() {
    let schema = quick_orientation_schema();
    let bounds = adaptive_agent_bounds(&schema).unwrap();
    assert_eq!(
        bounds,
        AdaptiveAgentBounds {
            subject_min: 1,
            subject_max: 2,
            method_min: 1,
            method_max: 2,
        }
    );
    let prompt =
        apply_agent_count_instruction("Router instructions".to_string(), Some(&schema)).unwrap();
    assert!(prompt.contains("2–4 specialists"));
    assert!(prompt.contains("1–2 method specialists"));

    let mut too_many_methods = valid_orientation();
    too_many_methods["review_plan"]["method_specialist_ids"] = serde_json::json!([
        "formal_proofs",
        "simulation_numerics",
        "reproducibility_software"
    ]);
    too_many_methods["review_plan"]["selection_notes"] = serde_json::json!([
        {"id": "subject_mathematics_pde", "reason": "The main result concerns a nonlinear PDE."},
        {"id": "formal_proofs", "reason": "The theorem and proof carry the contribution."},
        {"id": "simulation_numerics", "reason": "Numerical evidence supports the claim."},
        {"id": "reproducibility_software", "reason": "Software behavior is central."}
    ]);
    assert!(crate::pipeline::structured::validate(&schema, &too_many_methods).is_err());

    let mut fixed = schema;
    fixed[ADAPTIVE_AGENT_COUNT_KEY] = serde_json::json!(5);
    assert!(validate_schema_settings(&fixed).is_err());
}

#[test]
fn saved_profile_is_a_five_step_skeleton() {
    let steps = steps();
    assert_eq!(steps.len(), 5);
    assert!(steps.iter().all(|step| step.run_if.is_none()));
    assert!(steps
        .iter()
        .all(|step| step.tools == vec!["WebSearch".to_string()]));
    assert_eq!(
        steps
            .iter()
            .filter(|step| step.phase == Phase::Sequential)
            .map(|step| step.id.as_str())
            .collect::<Vec<_>>(),
        ["auto_synthesis", "auto_validate"]
    );
    let labels = steps
        .iter()
        .map(|step| step.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(&labels[3..], ["Consolidate Feedback", "Validate Feedback"]);
}

#[test]
fn materialization_inserts_only_selected_specialists_before_synthesis() {
    let config = configured_auto_review();
    let materialized = materialize_config(&config, &valid_orientation()).unwrap();
    assert_eq!(materialized.steps.len(), 7);
    assert_eq!(
        materialized
            .steps
            .iter()
            .map(|step| step.id.as_str())
            .collect::<Vec<_>>(),
        [
            "auto_contribution",
            "auto_consistency",
            "auto_exposition",
            "subject_mathematics_pde",
            "formal_proofs",
            "auto_synthesis",
            "auto_validate"
        ]
    );
    assert_eq!(
        synthesis_report_inputs(&materialized),
        [
            "auto_contribution",
            "auto_consistency",
            "auto_exposition",
            "subject_mathematics_pde",
            "formal_proofs",
        ]
    );
}

#[test]
fn materialized_specialists_foreground_their_selection_reason() {
    let config = configured_auto_review();
    let materialized = materialize_config(&config, &valid_orientation()).unwrap();
    let proofs = materialized
        .steps
        .iter()
        .find(|step| step.id == "formal_proofs")
        .unwrap();
    assert!(proofs
        .prompt
        .starts_with("# Method — Formal Proofs\n\n## Routing Context\n\n"));
    assert!(proofs
        .prompt
        .contains("> The theorem and proof carry the contribution."));
    assert!(proofs.prompt.contains("## Specialist Focus"));
    assert_specialist_prompt_contract(&proofs.prompt, "Method — Formal Proofs");

    // A runaway reason is condensed and capped rather than dominating the
    // specialist's context.
    let mut orientation = valid_orientation();
    orientation["review_plan"]["selection_notes"][1]["reason"] =
        serde_json::json!(format!("x {}", "long words ".repeat(400)));
    let materialized = materialize_config(&config, &orientation).unwrap();
    let proofs = materialized
        .steps
        .iter()
        .find(|step| step.id == "formal_proofs")
        .unwrap();
    let quoted = proofs
        .prompt
        .lines()
        .find(|line| line.starts_with("> "))
        .unwrap();
    assert!(quoted.chars().count() <= MAX_ROUTING_REASON_CHARS + 3);
    assert!(quoted.ends_with('…'));
}

#[test]
fn genre_classification_injects_shared_context_into_every_step() {
    let config = configured_auto_review();
    let mut orientation = valid_orientation();
    orientation["review_plan"]["genre"] = serde_json::json!("genre_survey_review");
    let materialized = materialize_config(&config, &orientation).unwrap();
    // A genre is context handed to all agents, never an extra reviewer.
    assert_eq!(materialized.steps.len(), 7);
    for step in materialized.steps.iter().filter(|step| step.enabled) {
        assert!(
            step.prompt
                .contains("classified this manuscript as: Survey & Review Article"),
            "{} is missing the genre context",
            step.id
        );
    }
    let proofs = materialized
        .steps
        .iter()
        .find(|step| step.id == "formal_proofs")
        .unwrap();
    assert!(proofs
        .prompt
        .starts_with("# Method — Formal Proofs\n\n## Document Genre\n\n"));
    assert!(proofs.prompt.contains("## Routing Context"));
    assert_specialist_prompt_contract(&proofs.prompt, "Method — Formal Proofs");

    // The default classification injects nothing.
    let plain = materialize_config(&config, &valid_orientation()).unwrap();
    assert!(plain
        .steps
        .iter()
        .all(|step| !step.prompt.contains("## Document Genre")));

    // Unknown classifications are rejected before any step runs.
    orientation["review_plan"]["genre"] = serde_json::json!("genre_unknown");
    assert!(materialize_config(&config, &orientation)
        .unwrap_err()
        .contains("unknown"));
}

#[test]
fn specialists_inherit_core_agent_and_model_configuration() {
    let mut config = configured_auto_review();
    for step in &mut config.steps {
        if step.phase == Phase::Parallel {
            step.agents = vec!["claude".to_string(), "codex".to_string()];
            step.model_overrides.insert(
                "claude:cli".to_string(),
                crate::settings::ModelSelection::Pinned {
                    model: "claude-opus-5".to_string(),
                },
            );
            step.effort_overrides
                .insert("codex:cli".to_string(), "high".to_string());
        }
    }
    let materialized = materialize_config(&config, &valid_orientation()).unwrap();
    for id in ["subject_mathematics_pde", "formal_proofs"] {
        let specialist = materialized
            .steps
            .iter()
            .find(|step| step.id == id)
            .unwrap();
        assert_eq!(specialist.phase, Phase::Parallel);
        assert_eq!(specialist.agents, ["claude", "codex"]);
        assert_eq!(
            specialist.model_overrides.get("claude:cli"),
            Some(&crate::settings::ModelSelection::Pinned {
                model: "claude-opus-5".to_string()
            })
        );
        assert_eq!(
            specialist.effort_overrides.get("codex:cli"),
            Some(&"high".to_string())
        );
    }
}

#[test]
fn routing_schema_and_semantics_reject_invalid_plans() {
    let schema = orientation_schema();
    let resolved = resolve_schema_catalogs(&schema).unwrap();
    let valid = valid_orientation();
    crate::pipeline::structured::validate(&resolved, &valid).unwrap();
    validate_contract_for_schema(&schema, &valid).unwrap();

    let mut wrong_category = valid.clone();
    wrong_category["review_plan"]["subject_specialist_ids"] = serde_json::json!(["formal_proofs"]);
    assert!(crate::pipeline::structured::validate(&resolved, &wrong_category).is_err());

    let mut missing_note = valid.clone();
    missing_note["review_plan"]["selection_notes"] = serde_json::json!([
        {"id": "subject_mathematics_pde", "reason": "The paper studies PDEs."}
    ]);
    assert!(validate_contract_for_schema(&schema, &missing_note).is_err());

    let mut unselected_note = valid;
    unselected_note["review_plan"]["selection_notes"][1]["id"] =
        serde_json::json!("statistical_validity");
    assert!(validate_contract_for_schema(&schema, &unselected_note).is_err());

    let mut parent_and_child = valid_orientation();
    parent_and_child["review_plan"]["subject_specialist_ids"] =
        serde_json::json!(["subject_mathematics_general", "subject_mathematics_pde"]);
    parent_and_child["review_plan"]["selection_notes"] = serde_json::json!([
        {"id": "subject_mathematics_general", "reason": "Broad mathematics fallback."},
        {"id": "subject_mathematics_pde", "reason": "The main result concerns PDEs."},
        {"id": "formal_proofs", "reason": "Proofs carry the result."}
    ]);
    assert!(validate_contract_for_schema(&schema, &parent_and_child)
        .unwrap_err()
        .contains("discipline fallback"));

    let mut fixed_count_schema = schema.clone();
    fixed_count_schema[ADAPTIVE_AGENT_COUNT_KEY] = serde_json::json!(3);
    assert!(
        validate_contract_for_schema(&fixed_count_schema, &valid_orientation())
            .unwrap_err()
            .contains("exactly 3 adaptive agents")
    );
    fixed_count_schema[ADAPTIVE_AGENT_COUNT_KEY] = serde_json::json!(2);
    validate_contract_for_schema(&fixed_count_schema, &valid_orientation()).unwrap();
}

#[test]
fn configured_count_is_injected_into_the_router_prompt() {
    let mut schema = orientation_schema();
    schema[ADAPTIVE_AGENT_COUNT_KEY] = serde_json::json!(4);
    let prompt = apply_agent_count_instruction(
        "Instructions\n\n<paper>paper text</paper>".to_string(),
        Some(&schema),
    )
    .unwrap();
    assert!(prompt.contains("Select exactly 4 total specialists"));
    assert!(prompt.find("Select exactly 4").unwrap() < prompt.find("<paper>").unwrap());

    schema[ADAPTIVE_AGENT_COUNT_KEY] = serde_json::json!(7);
    assert!(validate_schema_settings(&schema).is_err());
}

#[test]
fn every_discipline_fallback_materializes_and_the_run_stays_bounded() {
    let config = configured_auto_review();

    for subject in SUBJECTS
        .iter()
        .filter(|subject| subject.level == SubjectLevel::Discipline)
    {
        let mut orientation = valid_orientation();
        orientation["review_plan"]["primary_domain"] = serde_json::json!(subject.discipline_label);
        orientation["review_plan"]["subject_specialist_ids"] = serde_json::json!([subject.id]);
        orientation["review_plan"]["selection_notes"] = serde_json::json!([
            {"id": subject.id, "reason": "The paper's central claim belongs to this discipline."},
            {"id": "formal_proofs", "reason": "Formal derivations carry the result."}
        ]);
        let materialized = materialize_config(&config, &orientation).unwrap();
        assert_eq!(materialized.steps.len(), 7, "{}", subject.id);
        assert!(materialized.steps.iter().any(|step| step.id == subject.id));
    }

    let mut maximum = valid_orientation();
    maximum["review_plan"]["subject_specialist_ids"] = serde_json::json!([
        "subject_physics_quantum_information",
        "subject_computer_science_algorithms"
    ]);
    maximum["review_plan"]["method_specialist_ids"] = serde_json::json!([
        "formal_proofs",
        "algorithmic_ml",
        "simulation_numerics",
        "reproducibility_software"
    ]);
    maximum["review_plan"]["selection_notes"] = serde_json::json!([
        {"id": "subject_physics_quantum_information", "reason": "The physical result is about quantum information."},
        {"id": "subject_computer_science_algorithms", "reason": "The computational result is an algorithm."},
        {"id": "formal_proofs", "reason": "Formal guarantees carry the claim."},
        {"id": "algorithmic_ml", "reason": "Algorithmic performance is central."},
        {"id": "simulation_numerics", "reason": "Simulations support the finite-size results."},
        {"id": "reproducibility_software", "reason": "A custom implementation produces the evidence."}
    ]);
    let materialized = materialize_config(&config, &maximum).unwrap();
    assert_eq!(materialized.steps.len(), 11);
    assert_eq!(
        synthesis_report_inputs(&materialized),
        [
            "auto_contribution",
            "auto_consistency",
            "auto_exposition",
            "subject_physics_quantum_information",
            "subject_computer_science_algorithms",
            "formal_proofs",
            "algorithmic_ml",
            "simulation_numerics",
            "reproducibility_software",
        ]
    );
}
