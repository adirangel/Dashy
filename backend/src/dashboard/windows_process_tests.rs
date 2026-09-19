use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "dashy-launch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn file(&self, relative: &str) -> PathBuf {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "fixture").unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn portable_codex_without_an_alias_or_node_is_discovered() {
    let fixture = Fixture::new();
    let executable = fixture.file(&format!("Local/Microsoft/WinGet/Packages/OpenAI.Codex_Microsoft.Winget.Source_8wekyb3d8bbwe/codex-{CODEX_WINDOWS_TARGET}.exe"));
    let paths = windows_program_search_paths(
        None,
        None,
        None,
        Some(&fixture.0.join("Local")),
        None,
        None,
        None,
    );
    let launch = resolve_windows_program_from_paths(AllowedProgram::Codex, &paths).unwrap();
    assert_eq!(launch.executable, executable);
    assert!(launch.prefix_args.is_empty());
}

#[test]
fn native_provider_homes_are_discovered_without_inherited_path() {
    let fixture = Fixture::new();
    for (program, relative) in [
        (AllowedProgram::Codex, "Programs/OpenAI/Codex/bin/codex.exe"),
        (AllowedProgram::Claude, "Microsoft/WinGet/Packages/Anthropic.ClaudeCode_Microsoft.Winget.Source_8wekyb3d8bbwe/claude.exe"),
        (AllowedProgram::Grok, "Microsoft/WinGet/Packages/xAI.GrokBuild_Microsoft.Winget.Source_8wekyb3d8bbwe/grok.exe"),
    ] {
        let executable = fixture.file(relative);
        let paths = windows_program_search_paths(None, None, None, Some(&fixture.0), None, None, None);
        assert_eq!(resolve_windows_program_from_paths(program, &paths).unwrap().executable, executable);
    }
}

#[test]
fn codex_npm_native_layouts_do_not_require_node() {
    for package in [
        "node_modules/@openai/codex/node_modules/@openai/codex-win32-x64",
        "node_modules/@openai/codex-win32-x64",
        "node_modules/@openai/codex",
    ] {
        for suffix in ["bin/codex.exe", "codex/codex.exe"] {
            let fixture = Fixture::new();
            fixture.file("codex.cmd");
            let package = package.replace("codex-win32-x64", CODEX_WINDOWS_PACKAGE);
            let executable =
                fixture.file(&format!("{package}/vendor/{CODEX_WINDOWS_TARGET}/{suffix}"));
            let launch = resolve_windows_program_from_paths(
                AllowedProgram::Codex,
                std::slice::from_ref(&fixture.0),
            )
            .unwrap();
            assert_eq!(launch.executable, executable);
            assert!(launch.prefix_args.is_empty());
        }
    }
}

#[test]
fn legacy_claude_npm_wrapper_uses_node_with_a_separate_script_argument() {
    let fixture = Fixture::new();
    fixture.file("npm folder/claude.cmd");
    let script = fixture.file("npm folder/node_modules/@anthropic-ai/claude-code/cli.js");
    let node = fixture.file("node folder/node.exe");
    let paths = [fixture.0.join("npm folder"), fixture.0.join("node folder")];
    let launch = resolve_windows_program_from_paths(AllowedProgram::Claude, &paths).unwrap();
    assert_eq!(launch.executable, node);
    assert_eq!(launch.prefix_args.len(), 1);
    assert_eq!(PathBuf::from(&launch.prefix_args[0]), script);
    std::fs::remove_file(node).unwrap();
    assert!(resolve_windows_program_from_paths(AllowedProgram::Claude, &paths).is_none());
}

#[test]
fn persisted_path_wins_over_stale_process_entries_and_ignores_relative_entries() {
    let fixture = Fixture::new();
    fixture.file("old/claude.exe");
    let current = fixture.file("new/claude.exe");
    let old = std::env::join_paths([
        fixture.0.join("old"),
        PathBuf::from("relative"),
        PathBuf::new(),
    ])
    .unwrap();
    let new = std::env::join_paths([fixture.0.join("new")]).unwrap();
    let paths = windows_program_search_paths(Some(&old), Some(&new), None, None, None, None, None);
    assert!(paths.iter().all(|path| path.is_absolute()));
    assert_eq!(
        resolve_windows_program_from_paths(AllowedProgram::Claude, &paths)
            .unwrap()
            .executable,
        current
    );
}

#[tokio::test]
async fn invalid_executable_preserves_the_windows_error_code() {
    let fixture = Fixture::new();
    let executable = fixture.file("invalid.exe");
    let error = spawn_piped_launch(
        ProgramLaunch {
            executable,
            prefix_args: Vec::new(),
        },
        Vec::new(),
        false,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ProcessError::Spawn {
            os_code: Some(193 | 216)
        }
    ));
}

#[tokio::test]
async fn windows_child_replaces_a_missing_installer_working_directory() {
    let fixture = Fixture::new();
    let shell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32")
        .join("cmd.exe");
    let mut command = Command::new(shell);
    command.current_dir(fixture.0.join("removed-installer-directory"));
    configure_windows_working_directory(&mut command);
    let output = command
        .args(["/D", "/C", "echo", "fixture"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "fixture");
}

#[test]
fn portable_arm64_codex_is_visible_to_an_emulated_x64_dashy() {
    let fixture = Fixture::new();
    let executable = fixture.file("codex-aarch64-pc-windows-msvc.exe");
    let launch =
        resolve_windows_program_from_paths(AllowedProgram::Codex, std::slice::from_ref(&fixture.0))
            .unwrap();
    assert_eq!(launch.executable, executable);
    assert!(launch.prefix_args.is_empty());
}

#[test]
fn native_claude_npm_layouts_do_not_require_node() {
    for relative in [
        "node_modules/@anthropic-ai/claude-code/bin/claude.exe",
        "node_modules/@anthropic-ai/claude-code/node_modules/@anthropic-ai/claude-code-win32-x64/claude.exe",
        "node_modules/@anthropic-ai/claude-code-win32-x64/claude.exe",
        "node_modules/@anthropic-ai/claude-code-win32-arm64/claude.exe",
    ] {
        let fixture = Fixture::new();
        fixture.file("claude.cmd");
        let executable = fixture.file(relative);
        let launch = resolve_windows_program_from_paths(
            AllowedProgram::Claude, std::slice::from_ref(&fixture.0),
        ).unwrap();
        assert_eq!(launch.executable, executable);
        assert!(launch.prefix_args.is_empty());
    }
}

#[test]
fn claude_native_optional_dependency_wins_over_an_unreplaced_npm_stub() {
    let fixture = Fixture::new();
    fixture.file("claude.cmd");
    fixture.file("node_modules/@anthropic-ai/claude-code/bin/claude.exe");
    let executable = fixture.file("node_modules/@anthropic-ai/claude-code-win32-x64/claude.exe");
    let launch = resolve_windows_program_from_paths(
        AllowedProgram::Claude,
        std::slice::from_ref(&fixture.0),
    )
    .unwrap();
    assert_eq!(launch.executable, executable);
}
