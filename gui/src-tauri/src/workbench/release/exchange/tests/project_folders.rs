use super::*;

#[test]
fn new_project_import_requires_a_folder_in_preview_and_apply() {
    let (temp, store, ws, root) = fixture();
    populate(&store, &ws, &root);
    let path = temp.path().join("origin.pwex");
    export_to(&store, &ws, &path, selection());
    let no_folder = ExchangeTarget::NewWorkspace {
        name: "Copy".into(),
        root: None,
    };
    assert_eq!(
        preview_import(&store, &path.to_string_lossy(), &no_folder)
            .unwrap_err()
            .code,
        "invalid_input"
    );
    assert_eq!(
        import_exchange(
            &store,
            ImportExchangeRequest {
                path: path.to_string_lossy().into_owned(),
                target: no_folder,
                operation_id: "import-no-folder".into(),
            }
        )
        .unwrap_err()
        .code,
        "invalid_input"
    );
    assert_eq!(store.list_workspaces(true).unwrap().workspaces.len(), 1);
}
