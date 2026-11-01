use super::*;

#[derive(Default)]
struct RecordingEvents(std::sync::Mutex<Vec<(String, serde_json::Value)>>);

impl crate::emit::Events for RecordingEvents {
    fn emit_event(
        &self,
        event: &str,
        payload: serde_json::Value,
    ) -> Result<(), crate::emit::EmitError> {
        self.0.lock().unwrap().push((event.to_string(), payload));
        Ok(())
    }
}

#[tokio::test]
async fn captured_named_input_keeps_bytes_events_and_tempfile_lifetime() {
    let selected = tempfile::tempdir().unwrap();
    let original = selected.path().join("rubric.md");
    let text = "# Rubric\n\nUse exact evidence.\n";
    std::fs::write(&original, text).unwrap();
    let run_input_dir = tempfile::tempdir().unwrap();
    let run_store = tempfile::tempdir().unwrap();
    let mut run_writer =
        Some(crate::runs::RunWriter::create_in(run_store.path(), "named-input-fixture").unwrap());
    let mut config: crate::pipeline_config::PipelineConfig =
        crate::pipeline_config::ProfileData::new(
            "Named input fixture",
            Vec::new(),
            crate::pipeline_config::MergeConfig {
                enabled: false,
                prompt: String::new(),
                agents: Vec::new(),
            },
        )
        .into();
    config.extraction.method = "pdftotext".into();
    config.extraction.extra_inputs = vec![crate::pipeline_config::InputSlot {
        key: "rubric".into(),
        label: "Rubric".into(),
        required: true,
        ..Default::default()
    }];
    let provided = HashMap::from([(
        "rubric".to_string(),
        original.to_string_lossy().into_owned(),
    )]);
    let recorded = std::sync::Arc::new(RecordingEvents::default());
    let bus: crate::emit::EventBus = recorded.clone();
    let active = crate::run_scope::ActiveScope::begin("named-input-lifetime-fixture");
    crate::run_scope::scoped(active.scope().clone(), async {
        let _settings = crate::settings::freeze_for_run(crate::settings::Settings {
            pdf_extractor: "pdftotext".into(),
            ..Default::default()
        });
        let NamedInputs {
            resolved_inputs,
            persisted_inputs,
            extra_tmps,
            runtime_extra_sources,
            extra_sources,
        } = capture_named_inputs(
            &bus,
            &config,
            &provided,
            crate::pipeline::extract::MAX_FOLDER_COPY_FILES,
            &run_input_dir,
            &mut run_writer,
        )
        .await
        .unwrap();
        assert_eq!(extra_sources, provided);
        assert_eq!(persisted_inputs["rubric"], "context/inputs/00_rubric.txt");
        let durable = run_writer
            .as_ref()
            .unwrap()
            .dir()
            .join(&persisted_inputs["rubric"]);
        assert_eq!(std::fs::read_to_string(&durable).unwrap(), text);
        assert_eq!(
            std::fs::read_to_string(&runtime_extra_sources["rubric"]).unwrap(),
            text
        );
        assert_ne!(runtime_extra_sources["rubric"], extra_sources["rubric"]);
        let readable = std::path::PathBuf::from(&resolved_inputs["rubric"]);
        assert_eq!(std::fs::read_to_string(&readable).unwrap(), text);
        assert_eq!(extra_tmps.len(), 1);
        assert_eq!(
            *recorded.0.lock().unwrap(),
            vec![(
                "pipeline:log".to_string(),
                serde_json::json!({
                    "line": format!("Extra input 'rubric' extracted via text ({} chars)", text.len())
                })
            )]
        );
        drop(extra_tmps);
        assert!(!readable.exists());
        assert_eq!(std::fs::read_to_string(durable).unwrap(), text);
    })
    .await;
}
