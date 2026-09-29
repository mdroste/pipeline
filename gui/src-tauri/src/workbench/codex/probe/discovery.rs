//! Exercise the production discovery profiles with no authenticated model turn.
use super::*;

pub(super) async fn verify(
    stdin: &mut tokio::process::ChildStdin,
    reader: &mut BufReader<tokio::process::ChildStdout>,
    root: &Path,
    fixture: &Path,
    private: &Path,
    sibling: &Path,
) -> Result<(), String> {
    for (profile, writer) in [("discovery-inspect", false), ("discovery-edit", true)] {
        let id = json!(format!("{profile}-start"));
        let mut request = thread_start_request(id.clone(), root);
        request["params"]["permissions"] = json!(profile);
        request["params"]["approvalPolicy"] = json!("never");
        send(stdin, &request).await?;
        let started = response_for(reader, &id).await?;
        if started["approvalPolicy"] != "never"
            || started["approvalsReviewer"] != "user"
            || started["activePermissionProfile"]["id"] != profile
        {
            return Err(format!(
                "{profile} did not preserve autonomous approval/profile identity: {started}"
            ));
        }
        let thread = started["thread"]["id"]
            .as_str()
            .ok_or("Discovery thread missing")?;
        let id = json!(format!("{profile}-inject"));
        send(stdin,&json!({"method":"thread/inject_items","id":id,"params":{"threadId":thread,"items":[{"type":"message","role":"user","content":[{"type":"input_text","text":"Synthetic discovery qualification fixture. No response requested."}]}]}})).await?;
        response_for(reader, &id).await?;
        let id = json!(format!("{profile}-resume"));
        send(stdin,&json!({"method":"thread/resume","id":id,"params":{"threadId":thread,"permissions":profile,"cwd":root,"runtimeWorkspaceRoots":[root]}})).await?;
        let resumed = response_for(reader, &id).await?;
        let roots = resumed["runtimeWorkspaceRoots"]
            .as_array()
            .ok_or("Discovery runtime roots missing")?;
        if resumed["approvalPolicy"] != "never"
            || resumed["activePermissionProfile"]["id"] != profile
            || roots.len() != 1
            || !roots[0]
                .as_str()
                .is_some_and(|r| same_directory(Path::new(r), root))
        {
            return Err(format!(
                "{profile} resume changed its scope or approval policy"
            ));
        }
        for (label, path, allowed, write) in [
            ("read", fixture, true, false),
            ("private", private, false, false),
            ("sibling", sibling, false, false),
            ("write", root, false, true),
        ] {
            let target = if write {
                root.join(format!("{profile}-write"))
            } else {
                path.into()
            };
            let id = json!(format!("{profile}-{label}"));
            let mut request = command_exec_request(
                id.clone(),
                if write {
                    write_file_command(&target)
                } else {
                    read_file_command(&target)
                },
                root,
            );
            request["params"]["permissionProfile"] = json!(profile);
            send(stdin, &request).await?;
            let result = response_for(reader, &id).await?;
            let success = result["exitCode"] == 0;
            let expected = if write { writer } else { allowed };
            if success != expected || (write && target.exists() != writer) {
                return Err(format!(
                    "{profile} {label} did not enforce its filesystem scope: {result}"
                ));
            }
        }
    }
    Ok(())
}
