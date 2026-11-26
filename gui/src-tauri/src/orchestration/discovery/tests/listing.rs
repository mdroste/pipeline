//! Which runs a listing returns.
use super::*;

#[test]
fn listing_filters_by_project_only_when_one_is_named() {
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let p = fixture(Mode::Supervised, 50);
    storage::create(&s, &p.run, "start", "fingerprint").unwrap();
    assert_eq!(storage::list(&s, None, 0).unwrap().len(), 1);
    assert_eq!(storage::list(&s, Some("project"), 0).unwrap().len(), 1);
    assert!(storage::list(&s, Some("another-project"), 0)
        .unwrap()
        .is_empty());
}
