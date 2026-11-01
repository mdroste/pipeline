//! Keeps a Git read or fetch from acting on a folder's own config.
//!
//! A repository's config travels with the folder, so it can arrive in a zip, on a
//! shared drive, or in a replication package. Git would run its filters, signature
//! programs, and transports during an ordinary `status` or `log`, before the
//! researcher has chosen anything. A fetch is no different: looking at a remote is
//! not a decision to trust the folder. `ReadGuard` lists the config first, which
//! runs nothing, and overrides every `local` and `worktree` key that can start a
//! program, overwrite a file the folder chooses, or redirect a connection. A key
//! from `include.path` reports the scope of the file including it, so those are
//! covered too. The researcher's own settings keep their values, so Git LFS and
//! credential helpers still work.
//!
//! Setting the same key to an empty value is not enough for everything:
//! - `hook.<name>`: these hooks ignore `core.hooksPath`, and Git fails on an empty
//!   command, so a repository's hook gets `enabled=false`.
//! - `core.gitProxy`: the first value wins, so `GIT_PROXY_COMMAND` replaces it.
//! - `remote.<name>.uploadpack`: the first value wins. Reads get no transport at
//!   all, which also stops a partial clone from fetching a missing object, and
//!   every fetch here names the program itself.
//! - `remotes.<group>`: values add up, and a group starts one child fetch per
//!   member, which loses `--upload-pack`. A fetch of a group is refused.
//! - Remote helpers: an address such as `evil::x` starts `git remote-evil`, which
//!   is not a built-in command, so Git falls back to an alias of that name. A fetch
//!   allows only Git's own transports, and every alias the folder sets is emptied,
//!   because `ssh::x` passes that list and has no helper of its own to find.
//! - `remote.<name>.proxy` beats `http.proxy`, and `http.<url>.*` beats the plain
//!   key, so each is replaced under its own name.
//! - `gc.recentObjectsHook`: values add up, so automatic maintenance stays off.
//! - A key holding `=` would end at that `=` under `-c`, so it uses `--config-env`.
//!
//! `status` and `diff` normally start Git inside each submodule to look for
//! uncommitted work, under that repository's own config. Reads skip that, so a
//! submodule shows as changed only when its checked-out commit differs.
//!
//! An honest repository can need its own connection settings, such as a
//! `core.sshCommand` for a second account. A fetch ignores those too, and a failed
//! one says so through `fetch_advice`. Commit, push, and pull's merge run with the
//! repository's config: see `Git::act`.
use super::*;
use std::process::Command;

/// The transports a fetch may use. Anything else is a helper program.
const TRANSPORTS: &str = "https:http:ssh:git:file";

/// Forced on every guarded call, after everything else, whatever any scope says.
const ALWAYS: [&str; 7] = [
    "core.hooksPath=/dev/null",
    "core.fsmonitor=false",
    "log.showSignature=false",
    "diff.submodule=short",
    "protocol.ext.allow=never",
    "maintenance.auto=false",
    "gc.auto=0",
];

/// A key's section, subsection, and variable.
fn parts(key: &str) -> (&str, Option<&str>, &str) {
    let (section, rest) = key.split_once('.').unwrap_or((key, ""));
    match rest.rsplit_once('.') {
        Some((subsection, variable)) => (section, Some(subsection), variable),
        None => (section, None, rest),
    }
}

/// Whether a repository's value for `key` must not reach a read or a fetch. It
/// could start a program, overwrite a file (`http.saveCookies` writes to
/// `http.cookieFile`), or hand the researcher's credentials to another machine
/// (a proxy with checks turned off, or with a certificate authority of its own).
///
/// The rest of Git's command-valued keys are out of reach of those commands:
/// difftools, mergetools, and `submodule.<name>.update` belong to commands that
/// never run here, and Git reads `trace2.*` targets and
/// `uploadpack.packObjectsHook` only from the researcher's own config.
/// `core.hooksPath` and `core.fsmonitor` are in `ALWAYS`.
fn must_override(key: &str) -> bool {
    matches!(
        parts(key),
        ("alias", _, _)
            | (
                "core",
                None,
                "alternaterefscommand" | "askpass" | "editor" | "gitproxy" | "pager" | "sshcommand",
            )
            | ("credential", _, "helper")
            | ("diff", None, "external")
            | ("diff", Some(_), "command" | "textconv")
            | (
                "filter",
                Some(_),
                "clean" | "smudge" | "process" | "required"
            )
            | ("gpg", _, "program")
            | ("gpg", Some("ssh"), "defaultkeycommand")
            | ("hook", Some(_), _)
            | (
                "http",
                _,
                "cookiefile" | "proxy" | "savecookies" | "sslcainfo" | "sslcapath" | "sslverify",
            )
            | ("pager", None, _)
            | ("remote", Some(_), "proxy" | "vcs")
            | ("sequence", None, "editor")
    )
}

/// How to name a key that changes how Git reaches a remote, which an honest
/// repository may need. These are fixed names, so nothing a folder wrote reaches
/// the page, and a key's URL, which can hold a password, is never shown.
fn connection_setting(key: &str) -> Option<&'static str> {
    Some(match parts(key) {
        ("core", None, "askpass") => "core.askPass",
        ("core", None, "gitproxy") => "core.gitProxy",
        ("core", None, "sshcommand") => "core.sshCommand",
        ("credential", _, "helper") => "credential.helper",
        ("http", _, "cookiefile") => "http.cookieFile",
        ("http", _, "proxy") => "http.proxy",
        ("http", _, "sslcainfo") => "http.sslCAInfo",
        ("http", _, "sslcapath") => "http.sslCAPath",
        ("http", _, "sslverify") => "http.sslVerify",
        ("remote", Some(_), "proxy") => "remote.<name>.proxy",
        ("remote", Some(_), "uploadpack") => "remote.<name>.uploadpack",
        ("remote", Some(_), "vcs") => "remote.<name>.vcs",
        _ => return None,
    })
}

/// Whether a remote address names a helper program, as `hg::...` and `s3://...`
/// do, rather than one of the transports in `TRANSPORTS`.
fn names_helper(url: &str) -> bool {
    let end = url
        .find(|c: char| !c.is_ascii_alphanumeric() && !"+-.".contains(c))
        .unwrap_or(url.len());
    let (scheme, rest) = url.split_at(end);
    rest.starts_with("::")
        || (rest.starts_with("://")
            && !matches!(
                scheme.to_ascii_lowercase().as_str(),
                "file" | "git" | "git+ssh" | "http" | "https" | "ssh" | "ssh+git"
            ))
}

/// What replaces a repository's value before the researcher's own come back.
fn neutral(key: &str) -> &'static str {
    match parts(key) {
        // An empty command cannot start, which would end every SSH fetch.
        ("core", None, "sshcommand") => "ssh",
        ("filter", _, "required") | ("http", _, "savecookies") => "false",
        // Git reads an empty value as false.
        ("hook", _, "enabled") | ("http", _, "sslverify") => "true",
        _ => "",
    }
}

/// `filter.<name>.required`, for any key of that filter. Only the keys a
/// repository set are replaced, because Git skips `clean` once `process` has any
/// value, even an empty one. But a required filter with no command fails the read.
fn required_key(key: &str) -> Option<String> {
    let (name, _) = key.strip_prefix("filter.")?.rsplit_once('.')?;
    Some(format!("filter.{name}.required"))
}

/// The hook a `hook.<name>.<key>` setting belongs to.
fn hook_name(key: &str) -> Option<&str> {
    match parts(key) {
        ("hook", name, _) => name,
        _ => None,
    }
}

/// One setting from `git config --list`.
#[derive(Debug, PartialEq)]
struct Setting {
    /// From outside the repository: system, global, the command line.
    trusted: bool,
    key: String,
    value: Option<String>,
}

/// Reads one `key\nvalue` entry, or a bare `key` for a setting with no value.
fn setting(trusted: bool, entry: &[u8]) -> WorkbenchResult<Setting> {
    let (key, value) = match entry.iter().position(|byte| *byte == b'\n') {
        Some(at) => (&entry[..at], Some(&entry[at + 1..])),
        None => (entry, None),
    };
    let name = String::from_utf8_lossy(key);
    // A driver's name may hold any bytes, and an override has to spell it exactly.
    if !trusted && must_override(&name) && std::str::from_utf8(key).is_err() {
        return Err(WorkbenchError::invalid(
            "This folder's Git settings name a program in a way Pipeline can't safely skip.",
        ));
    }
    Ok(Setting {
        trusted,
        key: name.into_owned(),
        value: value.map(|value| String::from_utf8_lossy(value).into_owned()),
    })
}

/// Parses `git config --show-scope --list -z`.
fn scoped(output: &[u8]) -> WorkbenchResult<Vec<Setting>> {
    let mut fields = output.split(|byte| *byte == 0);
    let mut settings = Vec::new();
    while let (Some(scope), Some(entry)) = (fields.next(), fields.next()) {
        settings.push(setting(!matches!(scope, b"local" | b"worktree"), entry)?);
    }
    Ok(settings)
}

/// Parses `git config --list -z` from a scope that isn't trusted.
fn unscoped(output: &[u8]) -> WorkbenchResult<Vec<Setting>> {
    output
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| setting(false, entry))
        .collect()
}

/// The settings that apply to this folder, through `list`, which runs Git. Git
/// before 2.26 can't report scopes, so it reads the repository's files by name
/// and can't tell which of the researcher's own values to keep.
fn settings(list: &dyn Fn(&[&str]) -> WorkbenchResult<Vec<u8>>) -> WorkbenchResult<Vec<Setting>> {
    match list(&["config", "--show-scope", "--list", "-z"]) {
        Ok(output) => scoped(&output),
        Err(error) if error.message.contains("show-scope") => {
            let local = match list(&["config", "--local", "--includes", "--list", "-z"]) {
                Ok(output) => output,
                // Outside a repository, no repository's config applies.
                Err(error)
                    if error.message.contains("git repository")
                        || error.message.contains("git directory") =>
                {
                    Vec::new()
                }
                Err(error) => return Err(error),
            };
            // Git refuses this unless the repository can have settings per worktree.
            let worktree = list(&["config", "--worktree", "--list", "-z"]).unwrap_or_default();
            let mut settings = unscoped(&local)?;
            settings.extend(unscoped(&worktree)?);
            Ok(settings)
        }
        Err(error) => Err(error),
    }
}

/// The researcher's own values for `key`, in the order Git read them.
fn own_values<'a>(settings: &'a [Setting], key: &'a str) -> impl Iterator<Item = &'a Setting> {
    settings
        .iter()
        .filter(move |setting| setting.trusted && setting.key == key)
}

/// The arguments and environment that make one Git call safe in this folder.
#[derive(Debug, Default)]
pub(in super::super) struct ReadGuard {
    args: Vec<String>,
    env: Vec<(String, String)>,
    /// Connection settings of the repository's own that this guard ignores.
    connection: BTreeSet<&'static str>,
    /// Remote names that are also groups, in any scope.
    groups: BTreeSet<String>,
}

impl ReadGuard {
    /// Lists `git`'s folder. `git config` only reads files.
    pub(super) fn of(git: &Git) -> WorkbenchResult<Self> {
        Self::listed_by(&|args| {
            let mut command = git.command();
            command.args(args);
            capture_git(command, &git.root, GitLimits::default())
        })
    }

    pub(super) fn listed_by(
        list: &dyn Fn(&[&str]) -> WorkbenchResult<Vec<u8>>,
    ) -> WorkbenchResult<Self> {
        Ok(Self::from_settings(&settings(list)?))
    }

    fn from_settings(settings: &[Setting]) -> Self {
        let mut named = BTreeSet::new();
        let mut connection = BTreeSet::new();
        let groups = settings
            .iter()
            .filter_map(|setting| setting.key.strip_prefix("remotes."))
            .map(str::to_string)
            .collect();
        for setting in settings.iter().filter(|setting| !setting.trusted) {
            connection.extend(connection_setting(&setting.key));
            let helper = matches!(parts(&setting.key), ("remote", Some(_), "url"))
                && setting.value.as_deref().is_some_and(names_helper);
            if helper {
                connection.insert("remote.<name>.url");
            }
            if must_override(&setting.key) {
                named.extend(required_key(&setting.key));
                named.insert(setting.key.clone());
            }
        }
        let mut guard = Self {
            connection,
            groups,
            ..Self::default()
        };
        let mut hooks_off = BTreeSet::new();
        for key in &named {
            // A name the researcher also uses stays theirs, so it comes back below
            // with only their command and events. Any other hook is the folder's.
            if let Some(name) = hook_name(key) {
                let command = format!("hook.{name}.command");
                if own_values(settings, &command).next().is_none() {
                    hooks_off.insert(format!("hook.{name}.enabled"));
                    continue;
                }
            }
            guard.set(key, Some(neutral(key)));
            // What Git would have read had the repository not set this key.
            for own in own_values(settings, key) {
                guard.set(key, own.value.as_deref());
            }
        }
        for key in &hooks_off {
            guard.set(key, Some("false"));
        }
        if named.contains("core.gitproxy") {
            guard.env.push(("GIT_PROXY_COMMAND".into(), String::new()));
        }
        for always in ALWAYS {
            guard.args.extend(["-c".to_string(), always.to_string()]);
        }
        guard
    }

    fn set(&mut self, key: &str, value: Option<&str>) {
        if key.contains('=') {
            let name = format!("PIPELINE_GIT_VALUE_{}", self.env.len());
            self.args.push(format!("--config-env={key}={name}"));
            self.env.push((name, value.unwrap_or_default().to_string()));
        } else {
            self.args.push("-c".to_string());
            self.args.push(match value {
                Some(value) => format!("{key}={value}"),
                None => key.to_string(),
            });
        }
    }

    fn prepare(&self, command: &mut Command) {
        command
            .envs(self.env.iter().map(|(name, value)| (name, value)))
            .arg("--no-optional-locks")
            .args(&self.args);
    }

    /// Prepares `command` to read: it cannot change the repository, reach a
    /// remote, or start a program the folder's config names.
    pub(in super::super) fn read(&self, command: &mut Command, args: &[&str]) {
        command.env("GIT_ALLOW_PROTOCOL", "none");
        self.prepare(command);
        match args.split_first() {
            Some((verb, rest)) if matches!(*verb, "status" | "diff") => {
                command
                    .args([*verb, "--ignore-submodules=dirty"])
                    .args(rest);
            }
            _ => {
                command.args(args);
            }
        }
    }

    /// Prepares `command` to fetch `remote`, with `options` for that one fetch.
    /// Everything that keeps it safe is here, so no route can leave any of it out.
    /// Git's own transports are allowed, and nothing the folder's config names
    /// applies. Refused when `remote` is a group, which nothing here can make safe.
    pub(in super::super) fn fetch(
        &self,
        command: &mut Command,
        options: &[&str],
        remote: &str,
    ) -> WorkbenchResult<()> {
        if self.groups.contains(remote) {
            return Err(WorkbenchError::invalid(
                "This remote is a group of remotes (remotes.<group>). Pipeline doesn't fetch \
                 groups, because each member would use the folder's own Git settings. If you \
                 set this up yourself, run `git fetch` from a terminal in this folder.",
            ));
        }
        command
            .env("GIT_ALLOW_PROTOCOL", TRANSPORTS)
            .env("GIT_NO_LAZY_FETCH", "1");
        self.prepare(command);
        command
            .args([
                "fetch",
                "--upload-pack=git-upload-pack",
                "--no-recurse-submodules",
            ])
            .args(options)
            .args(["--", remote]);
        Ok(())
    }

    /// What a failed fetch should add when the folder's own config chooses how to
    /// connect. It works from a terminal, so Git's error alone would not explain it.
    pub(in super::super) fn fetch_advice(&self) -> Option<String> {
        if self.connection.is_empty() {
            return None;
        }
        let settings = Vec::from_iter(self.connection.iter().copied()).join(", ");
        Some(format!(
            "This folder's own Git settings choose how to connect ({settings}). Pipeline \
             doesn't use them here, because a folder you were sent could use them to run a \
             program. If you set them up yourself, run `git fetch` from a terminal in this \
             folder."
        ))
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    fn entry(trusted: bool, key: &str, value: &str) -> Setting {
        Setting {
            trusted,
            key: key.to_string(),
            value: Some(value.to_string()),
        }
    }

    fn overrides(settings: &[Setting]) -> Vec<String> {
        let guard = ReadGuard::from_settings(settings);
        let forced = 2 * ALWAYS.len();
        guard.args[..guard.args.len() - forced].to_vec()
    }

    #[test]
    fn every_key_a_repository_must_not_set_matches_whatever_its_name() {
        for key in [
            "core.sshcommand",
            "core.gitproxy",
            "core.askpass",
            "core.pager",
            "core.editor",
            "core.alternaterefscommand",
            "sequence.editor",
            "pager.log",
            "diff.external",
            "diff.any name.textconv",
            "diff.a.b.command",
            "filter.lfs.clean",
            "filter.a=b.smudge",
            "filter..process",
            "filter.x.required",
            "gpg.program",
            "gpg.openpgp.program",
            "gpg.x509.program",
            "gpg.ssh.program",
            "gpg.ssh.defaultkeycommand",
            "credential.helper",
            "credential.https://example.org.helper",
            "remote.origin.vcs",
            "remote.a.b.proxy",
            "alias.remote-ssh",
            "alias.remote-ssh.command",
            "hook.evil.command",
            "hook.evil.event",
            "hook.evil.enabled",
            "hook.a=b.c.command",
            "http.cookiefile",
            "http.savecookies",
            "http.proxy",
            "http.sslverify",
            "http.sslcainfo",
            "http.sslcapath",
            "http.https://example.org/.cookiefile",
            "http.https://example.org.savecookies",
            "http.https://example.org.proxy",
            "http.https://example.org.sslverify",
        ] {
            assert!(must_override(key), "{key}");
        }
        for key in [
            "user.name",
            "core.bare",
            "core.filemode",
            "diff.renames",
            "diff.tool",
            "filter.x.other",
            "gpg.format",
            "credential.usehttppath",
            "remote.origin.url",
            "include.path",
            "http.postbuffer",
            "http.https://example.org.useragent",
            "remote.origin.pushurl",
            "hook.jobs",
            "trace2.eventtarget",
        ] {
            assert!(!must_override(key), "{key}");
        }
    }

    #[test]
    fn scopes_decide_trust_and_a_setting_may_have_no_value() {
        let settings = scoped(
            b"system\0a.b\n1\0global\0c.d\n2\nlines\0unknown\0e.f\0local\0g.h\n3\0worktree\0i.j\n4\0command\0k.l\n5\0",
        )
        .unwrap();
        let read: Vec<_> = settings
            .iter()
            .map(|s| (s.trusted, s.key.as_str(), s.value.as_deref()))
            .collect();
        assert_eq!(
            read,
            [
                (true, "a.b", Some("1")),
                (true, "c.d", Some("2\nlines")),
                (true, "e.f", None),
                (false, "g.h", Some("3")),
                (false, "i.j", Some("4")),
                (true, "k.l", Some("5")),
            ]
        );
    }

    #[test]
    fn a_program_key_git_cannot_be_told_about_exactly_is_refused() {
        assert!(scoped(b"local\0filter.caf\xe9.clean\ntouch x\0").is_err());
        // Any other setting may hold those bytes, and so may the researcher's own.
        assert!(scoped(b"local\0branch.caf\xe9.remote\norigin\0").is_ok());
        assert!(scoped(b"global\0filter.caf\xe9.clean\ncat\0").is_ok());
    }

    #[test]
    fn a_repository_key_gives_way_to_the_researchers_own_values() {
        assert_eq!(
            overrides(&[
                entry(true, "credential.helper", "osxkeychain"),
                entry(true, "credential.helper", "store"),
                entry(false, "credential.helper", "!evil"),
            ]),
            [
                "-c",
                "credential.helper=",
                "-c",
                "credential.helper=osxkeychain",
                "-c",
                "credential.helper=store",
            ]
        );
        // Settings only the researcher made are left for Git to read.
        assert!(overrides(&[entry(true, "filter.lfs.clean", "git-lfs clean")]).is_empty());
    }

    #[test]
    fn a_repository_filter_loses_its_commands_and_stops_being_required() {
        assert_eq!(
            overrides(&[
                entry(false, "filter.evil.clean", "evil"),
                entry(false, "filter.evil.process", "evil"),
            ]),
            [
                "-c",
                "filter.evil.clean=",
                "-c",
                "filter.evil.process=",
                "-c",
                "filter.evil.required=false",
            ]
        );
        assert_eq!(
            overrides(&[entry(false, "filter.evil.required", "true")]),
            ["-c", "filter.evil.required=false"]
        );
    }

    #[test]
    fn a_researchers_filter_the_repository_also_sets_keeps_working() {
        let args = overrides(&[
            entry(true, "filter.lfs.clean", "git-lfs clean -- %f"),
            entry(true, "filter.lfs.required", "true"),
            entry(false, "filter.lfs.clean", "evil"),
        ]);
        let last = |key: &str| {
            args.iter()
                .rev()
                .find_map(|arg| arg.strip_prefix(key).map(str::to_string))
        };
        assert_eq!(
            last("filter.lfs.clean=").as_deref(),
            Some("git-lfs clean -- %f")
        );
        assert_eq!(last("filter.lfs.required=").as_deref(), Some("true"));
        // Any `process` value, even an empty one, would make Git skip `clean`.
        assert_eq!(last("filter.lfs.process="), None);
    }

    #[test]
    fn checks_a_repository_turned_off_come_back_on() {
        assert_eq!(
            overrides(&[
                entry(false, "http.savecookies", "true"),
                entry(false, "http.https://example.org.sslverify", "false"),
            ]),
            [
                "-c",
                "http.https://example.org.sslverify=true",
                "-c",
                "http.savecookies=false",
            ]
        );
    }

    #[test]
    fn a_repositorys_hook_is_turned_off_rather_than_emptied() {
        assert_eq!(
            overrides(&[
                entry(false, "hook.evil.event", "pre-push"),
                entry(false, "hook.evil.command", "evil"),
            ]),
            ["-c", "hook.evil.enabled=false"]
        );
    }

    #[test]
    fn a_hook_name_the_researcher_also_uses_keeps_only_their_values() {
        assert_eq!(
            overrides(&[
                entry(true, "hook.lint.command", "mine"),
                entry(true, "hook.lint.event", "pre-commit"),
                entry(false, "hook.lint.command", "evil"),
                entry(false, "hook.lint.event", "pre-push"),
                entry(false, "hook.lint.enabled", "false"),
            ]),
            [
                "-c",
                "hook.lint.command=",
                "-c",
                "hook.lint.command=mine",
                "-c",
                "hook.lint.enabled=true",
                "-c",
                "hook.lint.event=",
                "-c",
                "hook.lint.event=pre-commit",
            ]
        );
        // A name with no command of the researcher's runs nothing of theirs.
        assert_eq!(
            overrides(&[
                entry(true, "hook.lint.event", "pre-commit"),
                entry(false, "hook.lint.command", "evil"),
            ]),
            ["-c", "hook.lint.enabled=false"]
        );
    }

    #[test]
    fn a_group_is_refused_in_any_scope_under_its_exact_name() {
        for trusted in [true, false] {
            let guard = ReadGuard::from_settings(&[
                entry(trusted, "remotes.origin", "e1 e2"),
                entry(trusted, "remotes.a.b", "e1"),
            ]);
            let mut command = Command::new("git");
            for group in ["origin", "a.b"] {
                let refusal = guard.fetch(&mut command, &[], group).unwrap_err();
                assert!(refusal.message.contains("remotes.<group>"), "{refusal:?}");
                assert!(refusal.message.contains("terminal"), "{refusal:?}");
            }
            assert_eq!(command.get_args().count(), 0, "a refusal prepared Git");
            guard.fetch(&mut command, &[], "upstream").unwrap();
        }
    }

    #[test]
    fn every_fetch_carries_what_keeps_it_safe() {
        let guard = ReadGuard::from_settings(&[]);
        let mut command = Command::new("git");
        guard.fetch(&mut command, &["--quiet"], "origin").unwrap();
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let fetch = args.iter().position(|arg| arg == "fetch").unwrap();
        assert_eq!(
            args[fetch..],
            [
                "fetch",
                "--upload-pack=git-upload-pack",
                "--no-recurse-submodules",
                "--quiet",
                "--",
                "origin",
            ]
        );
        let env: Vec<_> = command.get_envs().collect();
        assert!(env.contains(&(
            std::ffi::OsStr::new("GIT_ALLOW_PROTOCOL"),
            Some(std::ffi::OsStr::new(TRANSPORTS))
        )));
        assert!(env.contains(&(
            std::ffi::OsStr::new("GIT_NO_LAZY_FETCH"),
            Some(std::ffi::OsStr::new("1"))
        )));
    }

    #[test]
    fn helper_addresses_are_told_from_gits_own_transports() {
        for url in [
            "hg::https://example.org/x",
            "ssh::x",
            "s3://bucket/x",
            "codecommit::us-east-1://x",
        ] {
            assert!(names_helper(url), "{url}");
        }
        for url in [
            "https://github.com/a/b.git",
            "HTTPS://github.com/a/b.git",
            "git@github.com:a/b.git",
            "ssh://git@[::1]/a.git",
            "git+ssh://host/a.git",
            "/srv/git/a.git",
            "file:///srv/git/a.git",
            "../a.git",
        ] {
            assert!(!names_helper(url), "{url}");
        }
    }

    #[test]
    fn fetch_advice_names_only_fixed_settings() {
        let advice = |settings: &[Setting]| ReadGuard::from_settings(settings).fetch_advice();
        assert_eq!(advice(&[entry(true, "core.sshcommand", "mine")]), None);
        assert_eq!(advice(&[entry(false, "user.name", "Ada")]), None);
        let text = advice(&[
            entry(false, "core.sshcommand", "ssh -i key"),
            entry(false, "remote.origin.uploadpack", "up"),
            entry(false, "http.https://a:secret@example.org.proxy", "p"),
        ])
        .unwrap();
        assert!(text.contains("core.sshCommand"), "{text}");
        assert!(text.contains("remote.<name>.uploadpack"), "{text}");
        assert!(text.contains("http.proxy"), "{text}");
        assert!(text.contains("terminal"), "{text}");
        assert!(
            !text.contains("secret") && !text.contains("ssh -i"),
            "{text}"
        );
    }

    #[test]
    fn ssh_keeps_working_and_a_proxy_is_replaced_by_the_environment() {
        let guard = ReadGuard::from_settings(&[
            entry(false, "core.sshcommand", "evil"),
            entry(false, "core.gitproxy", "evil"),
        ]);
        assert!(guard.args.contains(&"core.sshcommand=ssh".to_string()));
        assert_eq!(
            guard.env,
            [("GIT_PROXY_COMMAND".to_string(), String::new())]
        );
    }

    #[test]
    fn the_forced_settings_come_last() {
        let guard = ReadGuard::from_settings(&[
            entry(true, "core.fsmonitor", "true"),
            entry(false, "filter.evil.clean", "evil"),
        ]);
        let forced: Vec<&str> = guard.args[guard.args.len() - 2 * ALWAYS.len()..]
            .iter()
            .skip(1)
            .step_by(2)
            .map(String::as_str)
            .collect();
        assert_eq!(forced, ALWAYS);
    }
}
