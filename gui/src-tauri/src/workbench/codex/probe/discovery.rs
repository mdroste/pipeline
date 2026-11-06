//! Exercise the production discovery profiles with no authenticated model turn.
use super::*;
use crate::agent_runtime::codex::invocation::NativeThreadStart;
use crate::orchestration::store::Scope;
use crate::workbench::{discovery, research, store::Store};

pub(super) async fn verify(
    stdin: &mut tokio::process::ChildStdin,
    reader: &mut BufReader<tokio::process::ChildStdout>,
    root: &Path,
    fixture: &Path,
    private: &Path,
    sibling: &Path,
) -> Result<(), String> {
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let store = Store::open_at(&temporary.path().join("store")).map_err(|e| e.message)?;
    let session =
        discovery::source(&store, "probe", "Synthetic research", false).map_err(|e| e.message)?;
    let source = Scope {
        workspace_id: store
            .session_snapshot(&session)
            .map_err(|e| e.message)?
            .session
            .workspace_id,
        session_id: Some(session),
        ..Scope::default()
    };
    for (profile, writer) in [("discovery-inspect", false), ("discovery-edit", true)] {
        let (scope, _) = discovery::role(
            &store,
            &source,
            "probe",
            profile,
            writer,
            chrono::Utc::now().timestamp() + 3600,
        )
        .map_err(|e| e.message)?;
        let harness = research::resolve_harness(&store, scope.session_id.as_deref().unwrap())
            .map_err(|e| e.message)?;
        let id = json!(format!("{profile}-start"));
        let params = serde_json::to_value(NativeThreadStart {
            cwd: root,
            runtime_workspace_roots: &[root.display().to_string()],
            permissions: profile,
            approval_policy: "never",
            developer_instructions: Some(
                "Synthetic discovery fixture. No model response requested.",
            ),
            base_instructions: None,
            dynamic_tools: &harness.dynamic_tools,
            model: None,
            reasoning_effort: None,
            ephemeral: false,
            service_name: "pipeline_workbench",
            config: None,
        })
        .map_err(|e| e.to_string())?;
        let mut request = json!({"method":"thread/start","id":id,"params":params});
        request["params"]["modelProvider"] = json!("openai");
        request["params"]["allowProviderModelFallback"] = json!(false);
        request["params"]["approvalsReviewer"] = json!("user");
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
