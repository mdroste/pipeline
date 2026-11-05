//! Which workflow a re-run executes.
//!
//! A run is continued with the workflow it was started with, read from its
//! retained `context/workflow.json`, so selecting another workflow in the
//! meantime cannot change what Resume does. Regenerate differs in one case:
//! when the active workflow is the run's own, the installed copy runs so that
//! edited prompts take effect.
//!
//! Without a usable snapshot, the installed copy stands in only when its
//! fingerprint proves it is the workflow the run recorded. Only a run from
//! before workflows were recorded, which has neither, runs it unverified, and
//! the researcher is told first.

use crate::pipeline_config::{self, WorkflowDocument};
use crate::runs::RunManifest;

pub(super) const WORKFLOW_SNAPSHOT_PATH: &str = "context/workflow.json";

pub(super) struct RerunWorkflow {
    pub(super) document: WorkflowDocument,
    pub(super) profile_id: String,
    pub(super) source: String,
    /// What the researcher must know before this workflow runs: a run from
    /// before workflow snapshots uses an installed copy that may have changed.
    pub(super) notice: Option<String>,
}

/// The state of a run's retained workflow.
pub(super) enum SavedWorkflow {
    Missing,
    /// A file exists but could not be read. A transient read failure must not
    /// pass for a run that never saved one.
    Unreadable,
    Text(String),
}

impl SavedWorkflow {
    fn read(path: &std::path::Path, contents: Result<String, String>) -> Self {
        match contents {
            Ok(text) => Self::Text(text),
            Err(_)
                if matches!(
                    std::fs::symlink_metadata(path),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound
                ) =>
            {
                Self::Missing
            }
            Err(_) => Self::Unreadable,
        }
    }
}

fn installed_source(profile_id: &str) -> String {
    format!("profile:{profile_id}")
}

pub(super) fn resolve_rerun_workflow(
    parent: &RunManifest,
    active_profile: &str,
    only_failed: bool,
    saved: SavedWorkflow,
    load_installed: impl Fn(&str) -> Result<WorkflowDocument, String>,
) -> Result<RerunWorkflow, String> {
    let profile_id = parent.profile_id.as_str();
    if !only_failed && !profile_id.is_empty() && profile_id == active_profile {
        if let Ok(document) = load_installed(profile_id) {
            return Ok(RerunWorkflow {
                document,
                profile_id: profile_id.to_string(),
                source: installed_source(profile_id),
                notice: None,
            });
        }
    }
    let damaged = match saved {
        SavedWorkflow::Missing => false,
        SavedWorkflow::Unreadable => true,
        SavedWorkflow::Text(json) => match pipeline_config::parse_workflow_document_strict(&json) {
            Ok(document) => {
                return Ok(RerunWorkflow {
                    document,
                    profile_id: profile_id.to_string(),
                    source: if parent.workflow_source.is_empty() {
                        installed_source(profile_id)
                    } else {
                        parent.workflow_source.clone()
                    },
                    notice: None,
                });
            }
            Err(_) => true,
        },
    };
    let problem = if damaged {
        "This run's saved review design could not be read"
    } else {
        "This run did not save its review design"
    };
    if profile_id.is_empty() {
        return Err(format!("{problem}. Start a new Review on this document."));
    }
    let document = load_installed(profile_id).map_err(|_| {
        format!(
            "{problem}, and '{}' is no longer installed. Start a new Review on this document.",
            parent.profile_name
        )
    })?;
    let notice = if !parent.workflow_fingerprint.is_empty() {
        if document.fingerprint != parent.workflow_fingerprint {
            return Err(format!(
                "{problem}, and '{}' has changed since the run. Start a new Review on this document.",
                parent.profile_name
            ));
        }
        None
    } else if damaged {
        return Err(format!(
            "{problem}, and this run has no record to check '{}' against. Start a new Review on this document.",
            parent.profile_name
        ));
    } else {
        Some(format!(
            "This run is from an older version of Pipeline and did not save its review design. Continuing uses the current '{}', which may have changed since the run.",
            parent.profile_name
        ))
    };
    Ok(RerunWorkflow {
        document,
        profile_id: profile_id.to_string(),
        source: installed_source(profile_id),
        notice,
    })
}

/// The workflow a re-run of this stored run executes.
pub(super) fn load_rerun_workflow(
    run_id: &str,
    parent: &RunManifest,
    active_profile: &str,
    only_failed: bool,
) -> Result<RerunWorkflow, String> {
    let saved = SavedWorkflow::read(
        &crate::runs::runs_dir()?
            .join(run_id)
            .join(WORKFLOW_SNAPSHOT_PATH),
        super::read_run_file(run_id, WORKFLOW_SNAPSHOT_PATH),
    );
    resolve_rerun_workflow(
        parent,
        active_profile,
        only_failed,
        saved,
        pipeline_config::load_required_workflow_for,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow(name: &str, prompt: &str) -> WorkflowDocument {
        let template = pipeline_config::workflow_template().unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&template.canonical_json).unwrap();
        value["name"] = name.into();
        value["steps"][0]["prompt"] = prompt.into();
        pipeline_config::parse_workflow_document_strict(&value.to_string()).unwrap()
    }

    fn manifest(profile_id: &str, document: &WorkflowDocument) -> RunManifest {
        let mut manifest: RunManifest = serde_json::from_value(serde_json::json!({
            "run_id": "parent",
            "created": "2026-01-01T00:00:00Z",
            "input_path": "paper.pdf",
            "input_mode": "document",
            "profile_id": "",
            "profile_name": "",
            "provider": "claude",
            "artifacts": [],
            "status": "interrupted",
        }))
        .unwrap();
        manifest.profile_id = profile_id.to_string();
        manifest.profile_name = document.name.clone();
        manifest.workflow_fingerprint = document.fingerprint.clone();
        manifest.workflow_source = format!("profile:{profile_id}");
        manifest
    }

    fn resolve(
        parent: &RunManifest,
        active: &str,
        only_failed: bool,
        snapshot: Option<&str>,
        installed: &[(&str, &WorkflowDocument)],
    ) -> Result<RerunWorkflow, String> {
        let saved = match snapshot {
            Some(json) => SavedWorkflow::Text(json.to_string()),
            None => SavedWorkflow::Missing,
        };
        resolve_with(parent, active, only_failed, saved, installed)
    }

    fn resolve_with(
        parent: &RunManifest,
        active: &str,
        only_failed: bool,
        saved: SavedWorkflow,
        installed: &[(&str, &WorkflowDocument)],
    ) -> Result<RerunWorkflow, String> {
        resolve_rerun_workflow(parent, active, only_failed, saved, |id| {
            installed
                .iter()
                .find(|(installed_id, _)| *installed_id == id)
                .map(|(_, document)| (*document).clone())
                .ok_or_else(|| "not installed".to_string())
        })
    }

    #[test]
    fn resume_runs_the_saved_workflow_after_another_was_selected() {
        let review = workflow("Paper Review", "Review {input_text}");
        let other = workflow("Declaude", "Rewrite {input_text}");
        let parent = manifest("paper", &review);
        let resolved = resolve(
            &parent,
            "declaude",
            true,
            Some(&review.canonical_json),
            &[("paper", &review), ("declaude", &other)],
        )
        .unwrap();
        assert_eq!(resolved.document.fingerprint, review.fingerprint);
        assert_eq!(resolved.document.fingerprint, parent.workflow_fingerprint);
        assert_eq!(resolved.profile_id, "paper");
        assert_eq!(resolved.source, "profile:paper");
    }

    #[test]
    fn resume_ignores_edits_to_the_active_copy_of_its_workflow() {
        let saved = workflow("Paper Review", "Original {input_text}");
        let edited = workflow("Paper Review", "Edited {input_text}");
        let parent = manifest("paper", &saved);
        let resolved = resolve(
            &parent,
            "paper",
            true,
            Some(&saved.canonical_json),
            &[("paper", &edited)],
        )
        .unwrap();
        assert_eq!(resolved.document.fingerprint, saved.fingerprint);
    }

    #[test]
    fn regenerate_picks_up_edits_only_on_the_run_s_own_workflow() {
        let saved = workflow("Paper Review", "Original {input_text}");
        let edited = workflow("Paper Review", "Edited {input_text}");
        let other = workflow("Declaude", "Rewrite {input_text}");
        let parent = manifest("paper", &saved);
        let installed = [("paper", &edited), ("declaude", &other)];
        let same = resolve(
            &parent,
            "paper",
            false,
            Some(&saved.canonical_json),
            &installed,
        );
        assert_eq!(same.unwrap().document.fingerprint, edited.fingerprint);
        let elsewhere = resolve(
            &parent,
            "declaude",
            false,
            Some(&saved.canonical_json),
            &installed,
        );
        assert_eq!(elsewhere.unwrap().document.fingerprint, saved.fingerprint);
    }

    #[test]
    fn a_deleted_or_one_off_workflow_uses_the_saved_snapshot() {
        let saved = workflow("Portable", "Original {input_text}");
        let mut parent = manifest("ephemeral-abc", &saved);
        parent.workflow_source = "/tmp/portable.json".to_string();
        for only_failed in [true, false] {
            let resolved = resolve(
                &parent,
                "ephemeral-abc",
                only_failed,
                Some(&saved.canonical_json),
                &[],
            )
            .unwrap();
            assert_eq!(resolved.document.fingerprint, saved.fingerprint);
            assert_eq!(resolved.profile_id, "ephemeral-abc");
            assert_eq!(resolved.source, "/tmp/portable.json");
        }
    }

    #[test]
    fn without_a_snapshot_only_the_recorded_workflow_may_stand_in() {
        let saved = workflow("Paper Review", "Original {input_text}");
        let edited = workflow("Paper Review", "Edited {input_text}");
        let other = workflow("Declaude", "Rewrite {input_text}");
        let parent = manifest("paper", &saved);

        let same = resolve(&parent, "declaude", true, None, &[("paper", &saved)]).unwrap();
        assert_eq!(same.document.fingerprint, saved.fingerprint);
        assert!(same.notice.is_none());

        let changed = resolve(&parent, "paper", true, None, &[("paper", &edited)]);
        assert!(changed.err().unwrap().contains("has changed since the run"));

        let gone = resolve(&parent, "declaude", true, None, &[("declaude", &other)]);
        assert!(gone.err().unwrap().contains("no longer installed"));
    }

    #[test]
    fn a_damaged_snapshot_is_never_replaced_by_an_edited_workflow() {
        let saved = workflow("Paper Review", "Original {input_text}");
        let edited = workflow("Paper Review", "Edited {input_text}");
        let parent = manifest("paper", &saved);
        for damaged in [
            SavedWorkflow::Unreadable,
            SavedWorkflow::Text("{not a workflow".to_string()),
        ] {
            let error = resolve_with(&parent, "paper", true, damaged, &[("paper", &edited)])
                .err()
                .unwrap();
            assert!(error.contains("could not be read"), "{error}");
            assert!(error.contains("has changed since the run"), "{error}");
        }

        // Still the recorded workflow, so the run can continue.
        let unchanged = resolve_with(
            &parent,
            "paper",
            true,
            SavedWorkflow::Unreadable,
            &[("paper", &saved)],
        )
        .unwrap();
        assert_eq!(unchanged.document.fingerprint, saved.fingerprint);
        assert!(unchanged.notice.is_none());
    }

    #[test]
    fn only_a_run_with_no_record_at_all_runs_unverified_and_says_so() {
        let installed = workflow("Paper Review", "Current {input_text}");
        let mut legacy = manifest("paper", &installed);
        legacy.workflow_fingerprint.clear();

        let resolved = resolve(&legacy, "declaude", true, None, &[("paper", &installed)]).unwrap();
        let notice = resolved.notice.unwrap();
        assert!(notice.contains("older version of Pipeline"), "{notice}");
        assert!(notice.contains("Paper Review"), "{notice}");

        let error = resolve_with(
            &legacy,
            "declaude",
            true,
            SavedWorkflow::Unreadable,
            &[("paper", &installed)],
        )
        .err()
        .unwrap();
        assert!(error.contains("no record to check"), "{error}");
    }

    #[test]
    fn a_failed_read_counts_as_missing_only_when_no_file_is_there() {
        let directory = tempfile::tempdir().unwrap();
        let failed = || Err("read failed".to_string());
        assert!(matches!(
            SavedWorkflow::read(&directory.path().join("absent.json"), failed()),
            SavedWorkflow::Missing
        ));
        assert!(matches!(
            SavedWorkflow::read(directory.path(), failed()),
            SavedWorkflow::Unreadable
        ));
    }
}
