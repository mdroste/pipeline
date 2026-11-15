use super::*;

#[test]
fn older_portfolios_keep_their_start_identity_and_work_without_native_agents() {
    let p = fixture(Mode::Supervised, 50);
    let mut old = serde_json::to_value(&p.run).unwrap();
    old.as_object_mut().unwrap().remove("ideaSeeds");
    assert!(old["definition"].get("ideaAgents").is_none());
    let restored: Run = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(restored.next_generation_phase(), Phase::Generate);
    assert_eq!(
        old["definition"],
        serde_json::to_value(restored.definition).unwrap()
    );
}
