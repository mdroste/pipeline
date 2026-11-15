use super::*;

#[test]
fn every_profile_read_refreshes_inputs_without_rewriting_saved_settings() {
    let f = fixture();
    fs::write(f.root.join("main.tex"), "main").unwrap();
    let saved = save_build(&f.store, &f.ws, None, 0, serde_json::from_value(json!({"name":"Paper", "checkpointId":null, "directory":".", "rootDocument":"main.tex", "expectedPdf":"main.pdf", "engine":"latexmk", "timeoutSeconds":60, "inputs":["main.tex"], "advancedArgv":null, "automaticDependencies":true})).unwrap()).unwrap();
    let profile: research::ExecutionProfile =
        serde_json::from_value(saved.body["profile"].clone()).unwrap();
    fs::write(f.root.join("added.tex"), "new section").unwrap();
    let refreshed = research::get_execution_profile(&f.store, &profile.id).unwrap();
    assert!(refreshed.inputs.contains(&"added.tex".into()));
    assert_eq!(refreshed.revision, profile.revision);
    let saved_again = record(&f.store, &f.ws, &saved.id, "build").unwrap();
    assert_eq!(saved.body, saved_again.body);
}
