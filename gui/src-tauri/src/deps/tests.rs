use super::{
    antigravity_version_supported, cli_auth_status, cli_setup_recommendation, dependency_ready,
    parse_host_port, pdf_dependency_requirements, pdf_extraction_may_run, required_providers,
    resolve_command_in, windows_pathexts, CliAuthStatus, DepStatus,
};
#[cfg(unix)]
use super::{check_antigravity_auth, check_claude_auth, probe_resolved};
use std::ffi::{OsStr, OsString};
use std::path::Path;

fn write_fixture(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn assert_windows_path_eq(actual: &Path, expected: &Path) {
    assert!(
        actual
            .to_string_lossy()
            .eq_ignore_ascii_case(&expected.to_string_lossy()),
        "{} != {}",
        actual.display(),
        expected.display()
    );
}

fn write_node_shim(path: &Path, package_entrypoint: &str) {
    write_fixture(
        path,
        &format!(
            r#"@ECHO off
GOTO start
:find_dp0
SET dp0=%~dp0
EXIT /b
:start
SETLOCAL
CALL :find_dp0
IF EXIST "%dp0%\node.exe" (
  SET "_prog=%dp0%\node.exe"
) ELSE (
  SET "_prog=node"
  SET PATHEXT=%PATHEXT:;.JS;=;%
)
endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & "%_prog%"  "%dp0%\{package_entrypoint}" %*
"#
        ),
    );
}

#[test]
fn parse_ollama_default() {
    assert_eq!(
        parse_host_port("http://localhost:11434/v1"),
        Some(("localhost".into(), 11434))
    );
}

#[test]
fn parse_no_port_defaults_by_scheme() {
    assert_eq!(
        parse_host_port("http://myhost/v1"),
        Some(("myhost".into(), 80))
    );
    assert_eq!(
        parse_host_port("https://myhost/v1"),
        Some(("myhost".into(), 443))
    );
}

#[test]
fn parse_no_path() {
    assert_eq!(
        parse_host_port("http://127.0.0.1:1234"),
        Some(("127.0.0.1".into(), 1234))
    );
}

#[test]
fn parse_rejects_garbage() {
    assert_eq!(parse_host_port(""), None);
    assert_eq!(parse_host_port("localhost:11434"), None); // no scheme
    assert_eq!(parse_host_port("http://"), None);
    assert_eq!(parse_host_port("http://host:notaport/v1"), None);
}

#[test]
fn antigravity_version_gate_requires_headless_capable_release() {
    // 1.1.12 fixed --mode/--model/--effort in headless runs; older builds
    // silently ignore them and must fail the check.
    assert_eq!(antigravity_version_supported("1.1.12"), Some(true));
    assert_eq!(antigravity_version_supported("1.2.0"), Some(true));
    assert_eq!(antigravity_version_supported("2.0.0"), Some(true));
    assert_eq!(antigravity_version_supported("1.1.11"), Some(false));
    assert_eq!(antigravity_version_supported("0.9.9"), Some(false));
    // Unparseable output is unknown, not evidence either way.
    assert_eq!(antigravity_version_supported("1.1"), None);
    assert_eq!(antigravity_version_supported("1.1.12-beta"), None);
    assert_eq!(antigravity_version_supported(""), None);
    assert_eq!(antigravity_version_supported("dev"), None);
}

#[test]
fn active_profile_explicit_agents_are_required() {
    let settings = crate::settings::Settings {
        preferred_provider: "claude".to_string(),
        ..Default::default()
    };
    let explicit = crate::pipeline_config::StepConfig {
        id: "antigravity-review".to_string(),
        agents: vec!["antigravity".to_string()],
        ..Default::default()
    };
    let config = crate::pipeline_config::PipelineConfig {
        steps: vec![explicit],
        merge: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: crate::pipeline_config::ExtractionConfig {
            method: "pdftotext".to_string(),
            ..Default::default()
        },
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };

    let required = required_providers(&settings, Some(&config), false, None, None);
    assert_eq!(
        required,
        ["claude".to_string(), "antigravity".to_string()]
            .into_iter()
            .collect()
    );
}

#[test]
fn configured_usage_limit_fallback_is_required_at_preflight() {
    let settings = crate::settings::Settings {
        preferred_provider: "claude".to_string(),
        usage_limit_fallback_agent: "codex".to_string(),
        ..Default::default()
    };
    let required = required_providers(&settings, None, false, None, None);
    assert!(required.contains("claude"));
    assert!(required.contains("codex"));
}

#[test]
fn inherited_merge_default_is_required_for_multi_provider_steps() {
    let settings = crate::settings::Settings {
        default_merge_agent: "codex".to_string(),
        ..Default::default()
    };
    let config = crate::pipeline_config::PipelineConfig {
        steps: vec![crate::pipeline_config::StepConfig {
            id: "multi-provider-review".to_string(),
            agents: vec!["claude".to_string(), "antigravity".to_string()],
            ..Default::default()
        }],
        merge: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: crate::pipeline_config::ExtractionConfig {
            method: "pdftotext".to_string(),
            ..Default::default()
        },
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };

    let required = required_providers(&settings, Some(&config), false, None, None);
    assert!(required.contains("codex"));
}

#[test]
fn llm_extraction_requires_a_provider_only_for_a_possible_pdf_input() {
    let settings = crate::settings::Settings {
        preferred_provider: "claude".to_string(),
        pdf_extractor: "llm".to_string(),
        ..Default::default()
    };
    let mut config = crate::pipeline_config::PipelineConfig {
        steps: Vec::new(),
        merge: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };
    assert!(required_providers(
        &settings,
        Some(&config),
        false,
        Some("/papers/paper.pdf"),
        None,
    )
    .contains("claude"));
    assert!(required_providers(
        &settings,
        Some(&config),
        false,
        Some("/papers/paper.tex"),
        None,
    )
    .contains("claude"));

    config.extraction.input_mode = "none".to_string();
    assert!(required_providers(&settings, Some(&config), false, None, None).contains("claude"));
    config.extraction.input_mode = "folder".to_string();
    assert!(required_providers(&settings, Some(&config), false, None, None).contains("claude"));
}

#[test]
fn selected_named_pdf_inputs_participate_in_readiness() {
    let settings = crate::settings::Settings {
        preferred_provider: "claude".to_string(),
        pdf_extractor: "llm".to_string(),
        ..Default::default()
    };
    let mut config = crate::pipeline_config::PipelineConfig {
        steps: Vec::new(),
        merge: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: crate::pipeline_config::ExtractionConfig {
            input_mode: "none".to_string(),
            extra_inputs: vec![crate::pipeline_config::InputSlot {
                key: "appendix".to_string(),
                label: String::new(),
                mode: "document".to_string(),
                required: false,
            }],
            ..Default::default()
        },
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };
    let pdf = std::collections::HashMap::from([(
        "appendix".to_string(),
        "/papers/appendix.PDF".to_string(),
    )]);
    assert!(pdf_extraction_may_run(&config, None, Some(&pdf)));
    assert!(
        required_providers(&settings, Some(&config), false, None, Some(&pdf)).contains("claude")
    );

    let tex = std::collections::HashMap::from([(
        "appendix".to_string(),
        "/papers/appendix.tex".to_string(),
    )]);
    assert!(!pdf_extraction_may_run(&config, None, Some(&tex)));
    assert!(
        required_providers(&settings, Some(&config), false, None, Some(&tex)).contains("claude")
    );

    config.extraction.extra_inputs[0].mode = "folder".to_string();
    assert!(!pdf_extraction_may_run(&config, None, Some(&pdf)));
}

#[test]
fn pdf_dependencies_follow_the_effective_extractor() {
    let mut settings = crate::settings::Settings {
        preferred_provider: "claude".to_string(),
        pdf_extractor: "llm".to_string(),
        ..Default::default()
    };
    let mut config = crate::pipeline_config::PipelineConfig {
        steps: Vec::new(),
        merge: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };

    let llm =
        pdf_dependency_requirements(&settings, Some(&config), Some("/papers/paper.pdf"), None);
    assert!(llm.extraction);
    assert!(llm.pdftotext);
    assert!(!llm.pdftoppm);
    assert!(!llm.paddle_full);
    assert!(llm.configuration_ready);

    config.extraction.method = "paddleocr-vl-full".to_string();
    let full =
        pdf_dependency_requirements(&settings, Some(&config), Some("/papers/paper.pdf"), None);
    assert!(full.extraction);
    assert!(!full.pdftotext);
    assert!(!full.pdftoppm);
    assert!(full.paddle_full);
    assert!(full.configuration_ready);

    config.extraction.method = "llm".to_string();
    settings.preferred_provider = "local".to_string();
    let unsupported =
        pdf_dependency_requirements(&settings, Some(&config), Some("/papers/paper.pdf"), None);
    assert!(!unsupported.configuration_ready);

    let tex =
        pdf_dependency_requirements(&settings, Some(&config), Some("/papers/paper.tex"), None);
    assert!(!tex.extraction);
    assert!(tex.configuration_ready);
}

#[test]
fn windows_js_npm_shim_resolves_to_node_and_preserves_untrusted_arguments() {
    let temp = tempfile::tempdir().unwrap();
    let npm_dir = temp.path().join("npm global with spaces");
    let node_dir = temp.path().join("Node Runtime");
    let shim = npm_dir.join("codex.CMD");
    let entrypoint = npm_dir
        .join("node_modules")
        .join("@openai")
        .join("codex")
        .join("bin")
        .join("codex.js");
    let node = node_dir.join("node.EXE");
    write_fixture(&npm_dir.join("codex"), "#!/bin/sh\nexit 99\n");
    write_fixture(&entrypoint, "// fixture");
    write_fixture(&node, "fixture");
    write_node_shim(&shim, r"node_modules\@openai\codex\bin\codex.js");

    let directories = vec![npm_dir, node_dir];
    let resolved =
        resolve_command_in("codex", &directories, true, Some(OsStr::new(".cmd;.EXE"))).unwrap();

    assert_windows_path_eq(&resolved.discovered_path, &shim);
    assert_windows_path_eq(&resolved.program, &node);
    assert_eq!(
        resolved.prefix_args,
        vec![entrypoint.clone().into_os_string()]
    );

    // These strings would be shell syntax if concatenated into cmd /C.
    // The resolved representation keeps each one as an opaque argument.
    let caller_args = vec![
        "hello & whoami | more".to_string(),
        "a quoted \"value\"\nand a second line".to_string(),
        "%PATH% !PROMPT! ^ <input >output".to_string(),
    ];
    let command = resolved.command(&caller_args);
    assert_windows_path_eq(Path::new(command.get_program()), &node);
    let actual_args: Vec<OsString> = command.get_args().map(OsString::from).collect();
    let mut expected_args = vec![entrypoint.into_os_string()];
    expected_args.extend(caller_args.into_iter().map(OsString::from));
    assert_eq!(actual_args, expected_args);
}

#[test]
fn windows_pathext_order_and_case_are_honored() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("Provider Bin");
    let shim = bin.join("claude.CMD");
    let executable = bin.join("claude.EXE");
    let entrypoint = bin
        .join("node_modules")
        .join("@anthropic-ai")
        .join("claude-code")
        .join("cli.js");
    let node = bin.join("node.exe");
    write_fixture(&executable, "fixture");
    write_fixture(&entrypoint, "// fixture");
    write_fixture(&node, "fixture");
    write_node_shim(&shim, r"node_modules\@anthropic-ai\claude-code\cli.js");
    let directories = vec![bin];

    let exe_first =
        resolve_command_in("claude", &directories, true, Some(OsStr::new(".exe;.CMD"))).unwrap();
    assert_windows_path_eq(&exe_first.discovered_path, &executable);
    assert_windows_path_eq(&exe_first.program, &executable);

    let cmd_first =
        resolve_command_in("claude", &directories, true, Some(OsStr::new(".cmd;.EXE"))).unwrap();
    assert_windows_path_eq(&cmd_first.discovered_path, &shim);
    assert_windows_path_eq(&cmd_first.program, &node);
}

#[test]
fn windows_native_npm_shim_launches_target_without_node() {
    let temp = tempfile::tempdir().unwrap();
    let npm_dir = temp.path().join("npm global");
    let shim = npm_dir.join("claude.cmd");
    let executable = npm_dir
        .join("node_modules")
        .join("@anthropic-ai")
        .join("claude-code")
        .join("bin")
        .join("claude.exe");
    write_fixture(&executable, "native fixture");
    write_fixture(
        &shim,
        r#"@ECHO off
GOTO start
:find_dp0
SET dp0=%~dp0
EXIT /b
:start
SETLOCAL
CALL :find_dp0
"%dp0%\node_modules\@anthropic-ai\claude-code\bin\claude.exe"   %*
"#,
    );

    let resolved =
        resolve_command_in("claude", &[npm_dir], true, Some(OsStr::new(".CMD"))).unwrap();
    assert_windows_path_eq(&resolved.discovered_path, &shim);
    assert_windows_path_eq(&resolved.program, &executable);
    assert!(resolved.prefix_args.is_empty());
}

#[test]
fn windows_old_npm_shim_template_is_supported() {
    let temp = tempfile::tempdir().unwrap();
    let npm_dir = temp.path().join("legacy npm");
    let shim = npm_dir.join("codex.cmd");
    let node = npm_dir.join("node.exe");
    let entrypoint = npm_dir
        .join("node_modules")
        .join("@openai")
        .join("codex")
        .join("dist")
        .join("index.js");
    write_fixture(&node, "fixture");
    write_fixture(&entrypoint, "// fixture");
    write_fixture(
        &shim,
        r#"@IF EXIST "%~dp0\node.exe" (
  "%~dp0\node.exe" "%~dp0\node_modules\@openai\codex\dist\index.js" %*
) ELSE (
  node "%~dp0\node_modules\@openai\codex\dist\index.js" %*
)
"#,
    );

    let resolved =
        resolve_command_in("codex", &[npm_dir], true, Some(OsStr::new(".CMD;.EXE"))).unwrap();
    assert_windows_path_eq(&resolved.discovered_path, &shim);
    assert_windows_path_eq(&resolved.program, &node);
    assert_eq!(resolved.prefix_args, vec![entrypoint.into_os_string()]);
}

#[test]
fn windows_rejects_unrecognized_batch_shims() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    write_fixture(&bin.join("claude.cmd"), "@echo off\necho %*\n");
    assert!(resolve_command_in("claude", &[bin], true, Some(OsStr::new(".CMD")),).is_none());
}

#[test]
fn windows_pathext_defaults_match_create_process_conventions() {
    assert_eq!(windows_pathexts(None), vec![".COM", ".EXE", ".BAT", ".CMD"]);
    assert_eq!(
        windows_pathexts(Some(OsStr::new("cmd; .EXE; .cmd"))),
        vec![".cmd", ".EXE"]
    );
}

#[test]
#[cfg(unix)]
fn version_and_auth_probes_fail_closed() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin with spaces");
    let failing = bin.join("failing-provider");
    write_fixture(&failing, "#!/bin/sh\nexit 7\n");
    std::fs::set_permissions(&failing, std::fs::Permissions::from_mode(0o755)).unwrap();
    let command =
        resolve_command_in("failing-provider", std::slice::from_ref(&bin), false, None).unwrap();
    assert_eq!(probe_resolved(&command, &["--version"]), None);

    let empty_version = bin.join("empty-version");
    write_fixture(&empty_version, "#!/bin/sh\nexit 0\n");
    std::fs::set_permissions(&empty_version, std::fs::Permissions::from_mode(0o755)).unwrap();
    let command =
        resolve_command_in("empty-version", std::slice::from_ref(&bin), false, None).unwrap();
    assert_eq!(probe_resolved(&command, &["--version"]), None);

    let malformed_auth = bin.join("malformed-auth");
    write_fixture(&malformed_auth, "#!/bin/sh\nprintf 'not-json\\n'\n");
    std::fs::set_permissions(&malformed_auth, std::fs::Permissions::from_mode(0o755)).unwrap();
    let command = resolve_command_in("malformed-auth", &[bin], false, None).unwrap();
    assert_eq!(check_claude_auth(&command), None);
}

#[test]
#[cfg(unix)]
fn claude_auth_uses_json_state_even_when_signed_out_exits_nonzero() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    let signed_out = bin.join("signed-out");
    write_fixture(
        &signed_out,
        "#!/bin/sh\nprintf '{\"loggedIn\":false}\\n'\nexit 1\n",
    );
    std::fs::set_permissions(&signed_out, std::fs::Permissions::from_mode(0o755)).unwrap();
    let command = resolve_command_in("signed-out", &[bin], false, None).unwrap();
    assert_eq!(check_claude_auth(&command), Some(false));
}

#[test]
fn cli_auth_status_is_tri_state_and_only_applies_to_installed_clis() {
    assert_eq!(cli_auth_status(false, None), None);
    assert_eq!(
        cli_auth_status(true, Some(true)),
        Some(CliAuthStatus::SignedIn)
    );
    assert_eq!(
        cli_auth_status(true, Some(false)),
        Some(CliAuthStatus::SignedOut)
    );
    assert_eq!(cli_auth_status(true, None), Some(CliAuthStatus::Unknown));
}

#[test]
fn windows_cli_setup_uses_official_guides_instead_of_npm_commands() {
    for (product, command, expected_url) in [
        (
            "Claude Code",
            "npm install -g @anthropic-ai/claude-code",
            "https://code.claude.com/docs/en/installation",
        ),
        (
            "Codex CLI",
            "npm install -g @openai/codex",
            "https://developers.openai.com/codex/cli/",
        ),
        (
            "Antigravity CLI",
            "curl -fsSL https://antigravity.google/cli/install.sh | bash",
            "https://antigravity.google/docs/cli",
        ),
    ] {
        let (hint, url) = cli_setup_recommendation(product, command, false, true);
        assert!(!hint.contains("npm") && !hint.contains("curl"));
        assert!(hint.contains("Windows"));
        assert_eq!(url.as_deref(), Some(expected_url));
    }

    let (hint, url) = cli_setup_recommendation(
        "Antigravity CLI",
        "curl -fsSL https://antigravity.google/cli/install.sh | bash",
        true,
        true,
    );
    assert!(hint.starts_with("Upgrade"));
    assert!(!hint.contains("curl"));
    assert!(url.is_some());
}

#[test]
fn non_windows_cli_setup_retains_package_manager_commands() {
    let command = "npm install -g @openai/codex";
    let (hint, url) = cli_setup_recommendation("Codex CLI", command, false, false);
    assert!(hint.contains(command));
    assert_eq!(url, None);
}

#[test]
fn unverified_cli_authentication_fails_closed() {
    let status = |name: &str, found: bool| DepStatus {
        name: name.to_string(),
        found,
        version: String::new(),
        path: String::new(),
        required: true,
        hint: String::new(),
        help_url: None,
        authenticated: None,
        cli_auth_status: Some(CliAuthStatus::Unknown),
    };
    assert!(!dependency_ready(&status("Antigravity CLI", true)));
    assert!(!dependency_ready(&status("Antigravity CLI", false)));
    assert!(!dependency_ready(&status("Claude CLI", true)));
}

#[test]
#[cfg(unix)]
fn antigravity_auth_probe_reads_agy_models_deterministically() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");

    // Signed in: `agy models` exits 0 with the listing.
    let signed_in = bin.join("signed-in");
    write_fixture(&signed_in, "#!/bin/sh\nprintf 'gemini-3.5-flash\\n'\n");
    std::fs::set_permissions(&signed_in, std::fs::Permissions::from_mode(0o755)).unwrap();
    let command = resolve_command_in("signed-in", std::slice::from_ref(&bin), false, None).unwrap();
    assert_eq!(check_antigravity_auth(&command), Some(true));

    // Signed out: fast failure with the sign-in diagnostic on stderr.
    let signed_out = bin.join("signed-out");
    write_fixture(
        &signed_out,
        "#!/bin/sh\nprintf 'Error: Please sign in to view available models.\\n' >&2\nexit 1\n",
    );
    std::fs::set_permissions(&signed_out, std::fs::Permissions::from_mode(0o755)).unwrap();
    let command =
        resolve_command_in("signed-out", std::slice::from_ref(&bin), false, None).unwrap();
    assert_eq!(check_antigravity_auth(&command), Some(false));

    // Any other failure (network, crash) is unknown, not signed-out.
    let broken = bin.join("broken");
    write_fixture(
        &broken,
        "#!/bin/sh\nprintf 'dial tcp: timeout\\n' >&2\nexit 1\n",
    );
    std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o755)).unwrap();
    let command = resolve_command_in("broken", &[bin], false, None).unwrap();
    assert_eq!(check_antigravity_auth(&command), None);
}

#[test]
fn model_access_requires_every_required_provider() {
    let status = |name: &str, required: bool, found: bool, authenticated: Option<bool>| DepStatus {
        name: name.to_string(),
        found,
        version: String::new(),
        path: String::new(),
        required,
        hint: String::new(),
        help_url: None,
        authenticated,
        cli_auth_status: None,
    };
    // An available but unrequired provider must not mask a missing required
    // one — readiness is per required provider, not an OR over all of them.
    let missing_required = status("Claude CLI", true, false, None);
    let available_unrequired = status("Codex CLI", false, true, Some(true));
    let available_required = status("Antigravity CLI", true, true, Some(true));
    assert!(![&missing_required, &available_unrequired]
        .into_iter()
        .all(dependency_ready));
    assert!([&available_required, &available_unrequired]
        .into_iter()
        .all(dependency_ready));
    let missing_unrequired = status("Claude CLI", false, false, None);
    assert!([&missing_unrequired].into_iter().all(dependency_ready));
}

#[test]
fn required_native_dependencies_do_not_need_authentication_metadata() {
    let status = |found| DepStatus {
        name: "pdftotext".to_string(),
        found,
        version: String::new(),
        path: String::new(),
        required: true,
        hint: String::new(),
        help_url: None,
        authenticated: None,
        cli_auth_status: None,
    };
    assert!(dependency_ready(&status(true)));
    assert!(!dependency_ready(&status(false)));
}

#[cfg(unix)]
#[test]
fn unix_resolution_skips_non_executable_shadows() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let shadow = dir_a.path().join("claude");
    write_fixture(&shadow, "not a binary");
    std::fs::set_permissions(&shadow, std::fs::Permissions::from_mode(0o644)).unwrap();
    let real = dir_b.path().join("claude");
    write_fixture(&real, "#!/bin/sh\n");
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();

    let directories = vec![dir_a.path().to_path_buf(), dir_b.path().to_path_buf()];
    let resolved = resolve_command_in("claude", &directories, false, None).unwrap();
    assert_eq!(resolved.program, real);

    // With no executable candidate anywhere, resolution reports not-found
    // instead of a file that would fail to spawn with EACCES.
    let directories = vec![dir_a.path().to_path_buf()];
    assert!(resolve_command_in("claude", &directories, false, None).is_none());
}
