//! Turns Git's error output into what a researcher can act on. Text that reaches
//! the page first loses any credentials embedded in a remote address.
use super::*;
use regex::Regex;

const PREFIX: &str = "Git task setup failed: ";

fn redact(message: &str) -> String {
    static USERINFO: OnceLock<Regex> = OnceLock::new();
    USERINFO
        .get_or_init(|| Regex::new(r"://[^/@\s]+@").expect("static pattern"))
        .replace_all(message, "://")
        .into_owned()
}

/// Git's own words, without our prefix or embedded credentials.
pub(super) fn raw(message: &str) -> String {
    redact(message.trim_start_matches(PREFIX).trim())
}

/// The remote's host, unless its address has none to name.
fn host_of(remote: Option<&repository::RepositoryRemote>) -> Option<&str> {
    remote
        .map(|remote| remote.host.as_str())
        .filter(|host| !host.is_empty())
}

fn sign_in_help(remote: Option<&repository::RepositoryRemote>) -> String {
    match remote.and_then(|remote| remote.provider.as_deref()) {
        Some("github") => "GitHub did not accept your Git credentials. Install GitHub CLI and run `gh auth login`, or install Git Credential Manager, then try again.".to_string(),
        Some("bitbucket") => "Bitbucket did not accept your Git credentials. Bitbucket no longer takes app passwords: create an API token with repository access in your Atlassian account, or use an SSH key.".to_string(),
        _ => format!(
            "{} did not accept your Git credentials. Check that you can run this from a terminal in the project folder.",
            host_of(remote).unwrap_or("The remote")
        ),
    }
}

/// The advice for a failed action, or Git's own message when none fits.
pub(super) fn explain(message: &str, remote: Option<&repository::RepositoryRemote>) -> String {
    let text = raw(message);
    let has = |needle: &str| text.contains(needle);
    let host = host_of(remote).unwrap_or("the remote");
    if has("Stopped before Git finished") {
        "Stopped. Nothing further was sent or received.".to_string()
    } else if has("Git operation timed out") {
        "Git took too long and was stopped. Check the repository before you try again.".to_string()
    } else if has("Host key verification failed") {
        format!("Your computer has not trusted {host} yet. Connect once from a terminal with `ssh -T git@{host}`, then try again.")
    } else if has("Permission denied (publickey)")
        || has("Authentication failed")
        || has("could not read Username")
        || has("could not read Password")
        || has("terminal prompts disabled")
        || has("Invalid username or")
        || has("The requested URL returned error: 401")
        || has("The requested URL returned error: 403")
        || has("The requested URL returned error: 410")
    {
        sign_in_help(remote)
    } else if has("Repository not found") || has("does not appear to be a git repository") {
        format!("{host} has no such repository, or your account cannot see it. Check the address and your access.")
    } else if has("Could not resolve host")
        || has("Could not connect")
        || has("Failed to connect to")
        || has("Connection timed out")
        || has("Operation timed out")
    {
        format!("Could not reach {host}. Check your internet connection.")
    } else if has("GH001") || has("exceeds GitHub's file size limit") {
        "GitHub refused a file over its 100 MB limit. Use Undo last commit, add the file to .gitignore, and commit again.".to_string()
    } else if rejected_push(&text) {
        // Pull appears only once the page knows about those commits.
        "The remote has commits you don't have yet. Check for updates in the … menu, then Pull and push again.".to_string()
    } else if has("would be overwritten") {
        "Your uncommitted changes would be overwritten. Commit or discard them first, then try again.".to_string()
    } else if has("refusing to merge unrelated histories") {
        "This repository and the remote have no history in common. Connect an empty remote repository instead.".to_string()
    } else if has("Please tell me who you are") || has("empty ident") {
        "Git needs your name and email before it can commit.".to_string()
    } else if has("gpg failed to sign") || has("failed to sign the data") {
        "Git could not sign this commit. Check your signing setup from a terminal.".to_string()
    } else if has("index.lock") {
        "Another Git program is using this repository. If none is running, delete the file .git/index.lock and try again.".to_string()
    } else if has("nothing to commit") || has("nothing added to commit") {
        "Git found nothing to commit in those files. Changes inside a submodule are committed in the submodule.".to_string()
    } else if has("dubious ownership") {
        repository_problem(&text).unwrap_or(text)
    } else {
        text
    }
}

fn rejected_push(text: &str) -> bool {
    text.contains("fetch first") || text.contains("non-fast-forward")
}

/// The advice for a failed first push. A branch with no upstream can't be pulled
/// from this page, so a remote with its own history needs a different answer.
pub(super) fn explain_publish(
    message: &str,
    remote: Option<&repository::RepositoryRemote>,
    branch: &str,
) -> String {
    if rejected_push(&raw(message)) {
        format!("This remote already has its own commits on {branch}, and this page can't combine them with yours. Choose Change remote… in the … menu to connect an empty repository, or combine the two from a terminal.")
    } else {
        explain(message, remote)
    }
}

/// Whether the failure comes from the stand-in for Git on a Mac with no developer
/// tools, rather than from the folder.
pub(super) fn needs_developer_tools(message: &str) -> bool {
    message.contains("xcode-select") || message.contains("xcrun: error")
}

/// Why Git could not read a folder, or None when it simply holds no repository.
pub(super) fn repository_problem(message: &str) -> Option<String> {
    if message.contains("dubious ownership") {
        return Some("Git refuses to read this folder because it belongs to another user account. Trust it from a terminal with `git config --global --add safe.directory <folder>`.".to_string());
    }
    (!message.contains("not a git repository")).then(|| raw(message))
}

#[cfg(test)]
mod error_tests {
    use super::*;

    fn remote(url: &str) -> repository::RepositoryRemote {
        repository::parse_remote("origin", url).unwrap()
    }

    #[test]
    fn credentials_in_remote_addresses_never_reach_the_page() {
        let text = explain(
            "Git task setup failed: fatal: unable to update https://someone:ghp_secret@example.org/a/b.git",
            None,
        );
        assert!(!text.contains("ghp_secret"), "{text}");
        assert!(text.contains("https://example.org/a/b.git"), "{text}");
    }

    #[test]
    fn sign_in_failures_name_what_each_host_accepts() {
        let failure = "remote: Invalid credentials\nfatal: Authentication failed for 'x'";
        assert!(explain(failure, Some(&remote("https://github.com/a/b"))).contains("gh auth login"));
        assert!(
            explain(failure, Some(&remote("https://bitbucket.org/a/b"))).contains("app passwords")
        );
        assert!(
            explain(failure, Some(&remote("https://git.overleaf.com/abc")))
                .contains("git.overleaf.com")
        );
    }

    #[test]
    fn a_rejected_push_asks_for_a_pull() {
        assert!(explain(
            "! [rejected] main -> main (fetch first)\nerror: failed to push",
            None
        )
        .starts_with("The remote has commits"));
    }

    #[test]
    fn a_remote_with_no_host_is_never_named_by_an_empty_string() {
        let remote = repository::RepositoryRemote {
            name: "origin".into(),
            host: String::new(),
            owner: String::new(),
            repo: String::new(),
            provider: None,
            web_url: None,
        };
        let refused = explain("fatal: Authentication failed for 'x'", Some(&remote));
        assert!(
            refused.starts_with("The remote did not accept"),
            "{refused}"
        );
        let offline = explain("fatal: Could not connect to server", Some(&remote));
        assert!(offline.contains("reach the remote."), "{offline}");
    }

    #[test]
    fn a_connection_that_timed_out_is_not_git_taking_too_long() {
        for failure in [
            "ssh: connect to host github.com port 22: Operation timed out\nfatal: Could not read from remote repository.",
            "ssh: connect to host github.com port 22: Connection timed out",
            "fatal: unable to access 'https://github.com/a/b/': Failed to connect to github.com port 443 after 75003 ms: Couldn't connect to server",
        ] {
            let text = explain(failure, Some(&remote("https://github.com/a/b")));
            assert_eq!(
                text, "Could not reach github.com. Check your internet connection.",
                "{failure}"
            );
        }
        let stopped = explain(
            "Git operation timed out; inspect task state before retrying",
            None,
        );
        assert!(stopped.starts_with("Git took too long"), "{stopped}");
    }

    #[test]
    fn the_developer_tools_stand_in_is_not_a_folder_problem() {
        for message in [
            "Git task setup failed: xcode-select: note: No developer tools were found, requesting install.",
            "Git task setup failed: xcrun: error: invalid active developer path (/Library/Developer/CommandLineTools), missing xcrun",
        ] {
            assert!(needs_developer_tools(message), "{message}");
        }
        assert!(!needs_developer_tools(
            "fatal: not a git repository (or any of the parent directories): .git"
        ));
    }
}
