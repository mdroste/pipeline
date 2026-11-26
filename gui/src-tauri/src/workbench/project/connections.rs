//! Explicit transport choice for an attached source tree. No credentials or remote I/O.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionSettings {
    pub provider: String,
    pub remote_name: Option<String>,
    pub web_url: Option<String>,
    pub active: bool,
}

pub fn connection(store: &Store, ws: &str) -> WorkbenchResult<Option<ProjectRecord>> {
    Ok(records(store, ws, "connection")?.into_iter().next())
}

pub fn save_connection(
    store: &Store,
    ws: &str,
    expected: i64,
    settings: ConnectionSettings,
) -> WorkbenchResult<ProjectRecord> {
    let gate = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.root_path().join("connections.lock"))
        .map_err(err)?;
    fs2::FileExt::try_lock_exclusive(&gate).map_err(|_| {
        WorkbenchError::conflict("Another connection is being changed; retry shortly")
    })?;
    if !["git", "github", "overleaf", "dropbox"].contains(&settings.provider.as_str()) {
        return Err(WorkbenchError::invalid(
            "Choose Git, GitHub, Overleaf, or a local Dropbox folder",
        ));
    }
    if !settings.active {
        let old = connection(store, ws)?
            .ok_or_else(|| WorkbenchError::invalid("No connection to disconnect"))?;
        let mut body = old.body;
        body["active"] = json!(false);
        body["capabilities"] = json!([]);
        body["disconnectedAt"] = json!(now());
        return put(store, ws, &old.id, "connection", expected, &body);
    }
    let directory = root(store, ws)?;
    let identity = crate::workbench::store::root_identity(&directory)?;
    let mut remote_identity = Value::Null;
    let mut observed_revisions = Value::Null;
    if let Some(url) = &settings.web_url {
        let parsed = reqwest::Url::parse(url)
            .map_err(|_| WorkbenchError::invalid("Enter an HTTPS project link"))?;
        let host = parsed.host_str().unwrap_or("");
        let valid_host = match settings.provider.as_str() {
            "dropbox" => ["www.dropbox.com", "dropbox.com"].contains(&host),
            "overleaf" => ["www.overleaf.com", "overleaf.com"].contains(&host),
            "github" => host == "github.com",
            _ => false,
        };
        if parsed.scheme() != "https"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || !valid_host
        {
            return Err(WorkbenchError::invalid(
                "Use the provider's HTTPS project link without sign-in details",
            ));
        }
    }
    if settings.active {
        if settings.provider == "dropbox"
            && directory.join(".pipeline-tasks").is_dir()
            && fs::read_dir(directory.join(".pipeline-tasks"))
                .map_err(err)?
                .next()
                .is_some()
        {
            return Err(WorkbenchError::conflict("This folder contains older Pipeline task copies in .pipeline-tasks. Finish or discard those copies and move any retained folder outside Dropbox before connecting. New copies use private app storage."));
        }
        if store.root_path().starts_with(&directory) {
            return Err(WorkbenchError::invalid(
                "Move Pipeline's app storage outside the connected source folder first",
            ));
        }
        if let Some(old) = connection(store, ws)? {
            if old.body["active"] == true && old.body["provider"] != settings.provider {
                return Err(WorkbenchError::conflict(
                    "Disconnect the active transport before choosing another for this source tree",
                ));
            }
        }
        // A second project can attach a parent/subfolder of the same source tree.
        let c = store.connection()?;
        let mut query = c.prepare("SELECT body_json FROM project_records WHERE kind='connection' AND workspace_id<>?1 AND json_extract(body_json,'$.active')=1").map_err(err)?;
        let rows = query
            .query_map([ws], |row| row.get::<_, String>(0))
            .map_err(err)?;
        for row in rows {
            let body: Value = serde_json::from_str(&row.map_err(err)?).map_err(err)?;
            if let Some(other) = body["localRoot"].as_str() {
                let other = Path::new(other);
                if directory.starts_with(other) || other.starts_with(&directory) {
                    return Err(WorkbenchError::conflict(
                        "Another project already manages a transport for this source tree",
                    ));
                }
            }
        }
        if settings.provider != "dropbox" {
            let status = repository::inspect(&directory)
                .ok_or_else(|| WorkbenchError::invalid("Attach an existing Git clone first"))?;
            let selected = status
                .remote
                .ok_or_else(|| WorkbenchError::invalid("Select a Git remote first"))?;
            if settings.remote_name.as_deref() != Some(&selected.name) {
                return Err(WorkbenchError::conflict(
                    "Select this remote in Version control before connecting",
                ));
            }
            if settings.provider != "git"
                && selected.provider.as_deref() != Some(&settings.provider)
            {
                return Err(WorkbenchError::invalid(
                    "The selected remote does not belong to this provider",
                ));
            }
            remote_identity =
                json!({"host":selected.host,"owner":selected.owner,"repo":selected.repo});
            observed_revisions = json!({"local":status.head,"remote":status.remote_head,"fetchedAt":status.fetched_at,"pendingConflicts":status.conflicts});
        }
    }
    let capabilities = if settings.provider == "dropbox" {
        vec!["open", "read", "compare", "import"]
    } else {
        vec!["open", "read", "fetch", "compare", "import", "publish"]
    };
    put(
        store,
        ws,
        &format!("connection_{ws}"),
        "connection",
        expected,
        &json!({
            "provider": settings.provider, "remoteName": settings.remote_name, "webUrl": settings.web_url,
            "remoteIdentity": remote_identity,
            "observedRevisions": observed_revisions,
            "active": settings.active, "localRoot": directory.to_string_lossy(), "rootIdentity": identity,
            "capabilities": capabilities, "lastObservedAt": now(),
            "basis": if settings.provider == "dropbox" { "Local folder managed by Dropbox. Pipeline cannot establish upload completion or cloud revision." } else { "Local Git clone. Fetch, compare, pull, and publish are separate user actions." }
        }),
    )
}

/// Dropbox owns synchronization for its source tree. Local commits remain available.
pub fn check_transport_action(
    store: &Store,
    ws: &str,
    action: &version_control::Action,
) -> WorkbenchResult<()> {
    if let Some(connection) = connection(store, ws)? {
        if connection.body["active"] == true {
            if connection.body["rootIdentity"]
                != crate::workbench::store::root_identity(&root(store, ws)?)?
            {
                return Err(WorkbenchError::conflict(
                    "The connected folder changed; disconnect and reconnect its mapping",
                ));
            }
            if connection.body["provider"] == "dropbox"
                && matches!(
                    action,
                    version_control::Action::Pull
                        | version_control::Action::Push
                        | version_control::Action::Clone { .. }
                )
            {
                return Err(WorkbenchError::conflict("Dropbox manages this source tree. Disconnect it before using Git to pull or publish here."));
            }
            if matches!(
                action,
                version_control::Action::Pull | version_control::Action::Push
            ) {
                let selected = repository::inspect(&root(store, ws)?)
                    .and_then(|status| status.remote)
                    .ok_or_else(|| {
                        WorkbenchError::conflict("The connected remote is unavailable")
                    })?;
                if connection.body["remoteName"] != selected.name
                    || connection.body["remoteIdentity"]
                        != json!({"host":selected.host,"owner":selected.owner,"repo":selected.repo})
                {
                    return Err(WorkbenchError::conflict("The remote destination changed. Disconnect and reconnect the intended repository before pulling or publishing."));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn private_copy_parent(store: &Store, ws: &str) -> PathBuf {
    store.root_path().join("project-copies").join(ws)
}
