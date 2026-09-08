//! Units regression coverage.

use super::*;

#[test]
fn build_units_single_agent_has_empty_suffix() {
    let step = make_step("s", Phase::Parallel);
    let settings = crate::settings::Settings::default();
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let units = build_units(&step, &settings, "/tmp/x.pdf", None, &bus).unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].suffix, ""); // bare step id, no composite key
    assert_eq!(units[0].agent, settings.preferred_provider);
}

#[test]
fn build_units_multi_agent_keys_by_agent() {
    let mut step = make_step("s", Phase::Parallel);
    step.agents = vec!["claude".into(), "antigravity".into()];
    let settings = crate::settings::Settings::default();
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let units = build_units(&step, &settings, "/tmp/x.pdf", None, &bus).unwrap();
    assert_eq!(units.len(), 2);
    assert_eq!(units[0].suffix, "claude");
    assert_eq!(units[1].suffix, "antigravity");
}

#[test]
fn fan_out_builds_item_by_agent_cartesian_product() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("a")).unwrap();
    std::fs::create_dir_all(temp.path().join("b")).unwrap();
    std::fs::write(temp.path().join("a/note.md"), "a").unwrap();
    std::fs::write(temp.path().join("b/note.md"), "b").unwrap();

    let mut step = make_step("s", Phase::Parallel);
    step.agents = vec!["claude".into(), "antigravity".into()];
    step.for_each = Some(crate::pipeline_config::ForEach {
        glob: "**/*.md".into(),
        max: 10,
        artifact: None,
    });
    let settings = crate::settings::Settings::default();
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    let units = build_units(&step, &settings, temp.path().to_str().unwrap(), None, &bus).unwrap();

    assert_eq!(units.len(), 4);
    assert_eq!(units.iter().filter(|u| u.agent == "claude").count(), 2);
    assert_eq!(units.iter().filter(|u| u.agent == "antigravity").count(), 2);
    let item_keys: std::collections::HashSet<&str> =
        units.iter().map(|u| u.item_suffix.as_str()).collect();
    assert_eq!(item_keys.len(), 2, "duplicate basenames need distinct keys");
    assert!(units.iter().all(|u| u.merge_agents));
}

#[test]
fn fan_out_with_empty_source_root_matches_nothing() {
    let mut step = make_step("s", Phase::Parallel);
    step.for_each = Some(crate::pipeline_config::ForEach {
        glob: "**/*.md".into(),
        max: 10,
        artifact: None,
    });
    let settings = crate::settings::Settings::default();
    let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
    // An empty source root must not fall back to scanning the process cwd.
    let units = build_units(&step, &settings, "", None, &bus).unwrap();
    assert!(units.is_empty());
}

#[test]
fn step_file_keys_are_deterministic_and_collision_resistant() {
    assert_eq!(step_slug("technical"), step_slug("technical"));
    assert!(step_slug("technical").starts_with("technical--"));
    assert_ne!(step_slug("a.b"), step_slug("a_b"));
    assert_ne!(step_slug("technical"), step_slug("technical/claude"));
}
