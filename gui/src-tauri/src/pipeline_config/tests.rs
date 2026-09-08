use super::*;

// ── validate_unique_step_ids ───────────────────────────────────

fn step_with_id(id: &str) -> StepConfig {
    StepConfig {
        id: id.to_string(),
        label: id.to_string(),
        phase: Phase::Parallel,
        ..Default::default()
    }
}

fn mark_builtin_catalog_through_v6(dir: &Path) {
    for (marker, content) in [
        (
            ".builtin-catalog-v3",
            b"paper-and-code-profile-catalog\n".as_slice(),
        ),
        (
            ".builtin-catalog-v4",
            b"clean-terminal-report-prompts\n".as_slice(),
        ),
        (
            ".builtin-catalog-v6",
            b"explicit-step-artifact-context\n".as_slice(),
        ),
    ] {
        fs::write(dir.join(marker), content).unwrap();
    }
}

// ── validate_dependencies ──────────────────────────────────────

fn step_dep(id: &str, deps: &[&str]) -> StepConfig {
    StepConfig {
        id: id.to_string(),
        label: id.to_string(),
        after: deps.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

fn bundle_profile(id: &str, steps: Vec<StepConfig>) -> ProfileExport {
    ProfileExport {
        id: id.into(),
        name: id.into(),
        steps,
        merge: MergeConfig::default(),
        outputs: OutputConfig::default(),
        context_cache: ContextCacheConfig::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: ExtractionConfig::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    }
}

// ── slugify ────────────────────────────────────────────────────

// ── validate_profile_id ────────────────────────────────────────

// ── Legacy migration ───────────────────────────────────────────

// ── sanitize_step_id ──────────────────────────────────────────

mod graph;

mod identifiers;

mod migrations;

mod profiles;
