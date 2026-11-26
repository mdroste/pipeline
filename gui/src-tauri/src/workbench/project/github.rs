//! Selected-repository collaboration through the researcher's installed GitHub CLI.
//! Commands never authenticate, push commits, merge, or download artifacts implicitly.
use super::git_process::{capture_git, GitLimits};
use super::version_control::Git;
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitHubRequest {
    pub expected_head: String,
    pub expected_repository: String,
    pub title: String,
    pub body: String,
    pub base: String,
}

fn selected(root: &Path) -> WorkbenchResult<(repository::RepositoryStatus, String)> {
    let status = repository::inspect(root)
        .ok_or_else(|| WorkbenchError::invalid("Open a GitHub clone first"))?;
    let remote = status
        .remote
        .as_ref()
        .filter(|remote| remote.provider.as_deref() == Some("github"))
        .ok_or_else(|| WorkbenchError::invalid("Select a github.com remote first"))?;
    Ok((status.clone(), format!("{}/{}", remote.owner, remote.repo)))
}

fn gh(root: &Path, args: &[&str]) -> WorkbenchResult<Vec<u8>> {
    let executable = crate::deps::find_on_path("gh").ok_or_else(|| WorkbenchError::invalid("Install GitHub CLI and sign in with gh auth login to use collaboration. Local Git remains available."))?;
    let mut command = std::process::Command::new(executable);
    command
        .args(args)
        .env("GH_HOST", "github.com")
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "")
        .env("NO_COLOR", "1");
    capture_git(
        command,
        root,
        GitLimits {
            timeout: Duration::from_secs(45),
            ..GitLimits::default()
        },
    )
    .map_err(|error| {
        WorkbenchError::invalid(
            error
                .message
                .replace("Git task setup failed:", "GitHub CLI:"),
        )
    })
}
fn json_gh(root: &Path, args: &[&str]) -> WorkbenchResult<Value> {
    serde_json::from_slice(&gh(root, args)?).map_err(err)
}

pub fn inspect(root: &Path) -> WorkbenchResult<Value> {
    let (status, repository) = selected(root)?;
    let head = status
        .head
        .ok_or_else(|| WorkbenchError::invalid("Make a commit before checking collaboration"))?;
    let branch = status
        .branch
        .ok_or_else(|| WorkbenchError::invalid("Choose a branch first"))?;
    let prs = json_gh(root, &["pr", "list", "--repo", &format!("github.com/{repository}"), "--head", &branch, "--state", "all", "--limit", "20", "--json", "number,title,url,state,isDraft,headRefOid,baseRefName,headRefName,statusCheckRollup,comments"])?;
    let issue_result = json_gh(
        root,
        &[
            "issue",
            "list",
            "--repo",
            &format!("github.com/{repository}"),
            "--state",
            "open",
            "--limit",
            "20",
            "--json",
            "id,number,title,body,url",
        ],
    );
    let (issues, issues_error) = match issue_result {
        Ok(issues) => (issues, None),
        Err(error) => (json!([]), Some(error.message)),
    };
    let checks = json_gh(
        root,
        &[
            "api",
            "--method",
            "GET",
            &format!("repos/{repository}/actions/runs?head_sha={head}&per_page=20"),
        ],
    );
    let (runs, checks_error) = match checks {
        Ok(checks) => (checks["workflow_runs"].clone(), None),
        Err(error) => (json!([]), Some(error.message)),
    };
    Ok(
        json!({"repository":repository,"branch":branch,"head":head,"checkedAt":now(),"pullRequests":prs,"issues":issues,"issuesError":issues_error,"runs":runs,"checksError":checks_error,"scope":"Selected branch and exact local commit. Up to 20 pull requests, 20 open issues, and 20 workflow runs. Pull request conversation comments only. Open a run to inspect CI artifacts; their contents are not downloaded."}),
    )
}

pub fn draft_pull_request(root: &Path, request: GitHubRequest) -> WorkbenchResult<String> {
    let (status, repository) = selected(root)?;
    if status.head.as_deref() != Some(&request.expected_head)
        || repository != request.expected_repository
    {
        return Err(WorkbenchError::conflict(
            "The selected repository or commit changed; inspect and prepare the pull request again",
        ));
    }
    let branch = status
        .branch
        .ok_or_else(|| WorkbenchError::invalid("Choose a branch first"))?;
    let git = Git::open(root).ok_or_else(|| WorkbenchError::invalid("Git is unavailable"))?;
    if request.base.is_empty()
        || request.base.starts_with('-')
        || git
            .read(&["check-ref-format", "--branch", &request.base])
            .is_err()
    {
        return Err(WorkbenchError::invalid("Enter a valid destination branch"));
    }
    if request.title.trim().is_empty()
        || request.title.len() > 256
        || request.body.len() > 64 * 1024
    {
        return Err(WorkbenchError::invalid(
            "Use a title through 256 bytes and a description through 64 KiB",
        ));
    }
    // Read the actual remote ref immediately before publication. A cached local
    // remote ref does not establish what commit the service will put in the PR.
    let escaped_branch = branch
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect::<String>();
    let remote = json_gh(
        root,
        &[
            "api",
            "--method",
            "GET",
            &format!("repos/{repository}/git/ref/heads/{escaped_branch}"),
        ],
    )?;
    if remote["object"]["sha"] != request.expected_head {
        return Err(WorkbenchError::conflict("Publish this exact branch commit to the selected GitHub repository first; its remote head differs"));
    }
    let mut body = tempfile::NamedTempFile::new().map_err(err)?;
    body.write_all(request.body.as_bytes()).map_err(err)?;
    body.flush().map_err(err)?;
    let output = gh(
        root,
        &[
            "pr",
            "create",
            "--draft",
            "--repo",
            &format!("github.com/{repository}"),
            "--head",
            &branch,
            "--base",
            &request.base,
            "--title",
            &request.title,
            "--body-file",
            &body.path().to_string_lossy(),
        ],
    )?;
    let url = String::from_utf8(output).map_err(err)?.trim().to_string();
    if !url.starts_with(&format!("https://github.com/{repository}/pull/")) {
        return Err(WorkbenchError::invalid(
            "GitHub returned an unexpected result. Check the repository before retrying.",
        ));
    }
    Ok(url)
}

pub fn import_comment(
    store: &Store,
    ws: &str,
    snapshot_id: &str,
    comment_id: &str,
) -> WorkbenchResult<ProjectRecord> {
    let _guard = lock(store, ws)?;
    let snapshot = crate::workbench::desk::record(store, ws, snapshot_id)?;
    if snapshot.kind != "github_collaboration" {
        return Err(WorkbenchError::invalid(
            "Choose a checked GitHub collaboration snapshot",
        ));
    }
    let comment = snapshot.body["pullRequests"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|pr| pr["comments"].as_array().into_iter().flatten())
        .chain(snapshot.body["issues"].as_array().into_iter().flatten())
        .find(|comment| comment["id"].as_str() == Some(comment_id))
        .ok_or_else(|| WorkbenchError::invalid("Choose a comment from this checked snapshot"))?;
    let source = format!(
        "GitHub comment {comment_id}; snapshot {} ({})",
        snapshot.id, snapshot.content_hash
    );
    let key = format!(
        "github_comment_{}",
        hash(format!("{ws}:{}:{comment_id}", snapshot.body["repository"]).as_bytes())
    );
    if let Ok(old) = record(store, ws, &key, "github_comment") {
        return record(store, ws, old.body["taskId"].as_str().unwrap_or(""), "task");
    }
    let objective = format!(
        "{}\n\n{}\n{}",
        comment["body"].as_str().unwrap_or(""),
        comment["url"].as_str().unwrap_or(""),
        source
    );
    let task = tasks::create_task(
        store,
        ws,
        objective,
        None,
        vec!["Inspect and respond to the linked comment".into()],
        vec![],
    )?;
    put(
        store,
        ws,
        &key,
        "github_comment",
        0,
        &json!({"taskId":task.id,"snapshotId":snapshot.id,"snapshotHash":snapshot.content_hash,"comment":comment}),
    )?;
    Ok(task)
}
