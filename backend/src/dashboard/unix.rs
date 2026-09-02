//! Program resolution and visible terminal launches on macOS and Linux.
//!
//! A desktop-launched application does not inherit the user's shell `PATH`, so
//! Dashy resolves each provider CLI against the process `PATH`, the login shell's
//! `PATH`, and the fixed directories the official installers use, then spawns the
//! resolved executable directly. Nothing here reads provider configuration or
//! credentials; only executable files are looked up.

use std::{
    ffi::{OsStr, OsString},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

use super::process::{AllowedProgram, ProgramLaunch, VisibleProcessError};

/// How long the login shell may take to print its `PATH` before Dashy proceeds
/// without it. The shell runs once per process.
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(3);
const LOGIN_SHELL_MAX_OUTPUT: usize = 64 * 1024;
/// Provider logins and installs are interactive; give the user ample time before
/// the setup action is reported as not completed.
const VISIBLE_RUN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const VISIBLE_RUN_POLL: Duration = Duration::from_millis(250);
/// `sh` reports a missing or non-executable command with this status.
const SHELL_COMMAND_NOT_FOUND: i32 = 127;

static LOGIN_SHELL_PATH: OnceLock<Option<OsString>> = OnceLock::new();

/// Starts computing the login shell `PATH` in the background so the first
/// provider refresh does not wait for the shell.
pub fn prime_login_shell_path() {
    std::thread::Builder::new()
        .name("dashy-login-shell-path".into())
        .spawn(|| {
            let _ = login_shell_path();
        })
        .ok();
}

fn login_shell_path() -> Option<&'static OsStr> {
    LOGIN_SHELL_PATH
        .get_or_init(|| {
            let shell = std::env::var_os("SHELL")?;
            read_login_shell_path(Path::new(&shell), LOGIN_SHELL_TIMEOUT)
        })
        .as_deref()
}

/// Runs `$SHELL -l -c 'printf %s "$PATH"'` with a bounded wait and bounded output.
fn read_login_shell_path(shell: &Path, timeout: Duration) -> Option<OsString> {
    if !shell.is_absolute() || !shell.is_file() {
        return None;
    }
    let mut child = std::process::Command::new(shell)
        .args(["-l", "-c", "printf '%s' \"$PATH\""])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut output = Vec::new();
        let mut limited = stdout.take(LOGIN_SHELL_MAX_OUTPUT as u64 + 1);
        let _ = limited.read_to_end(&mut output);
        // Drain anything past the limit so a chatty profile cannot block the shell.
        let mut rest = limited.into_inner();
        let _ = std::io::copy(&mut rest, &mut std::io::sink());
        output
    });
    let started = Instant::now();
    let exited = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let output = reader.join().ok()?;
    if !exited || output.is_empty() || output.len() > LOGIN_SHELL_MAX_OUTPUT {
        return None;
    }
    // Profiles may print banners; keep only the last line, which is the PATH.
    let last_line = output
        .rsplit(|byte| *byte == b'\n')
        .find(|line| !line.is_empty())?;
    Some(OsString::from(
        String::from_utf8_lossy(last_line).into_owned(),
    ))
}

pub(crate) fn program_launch(program: AllowedProgram) -> Option<ProgramLaunch> {
    resolve_program_from_paths(program, &search_paths()).map(|executable| ProgramLaunch {
        executable,
        prefix_args: Vec::new(),
    })
}

/// The `PATH` handed to provider CLIs so their own helpers resolve too.
pub(crate) fn child_path_value() -> OsString {
    std::env::join_paths(search_paths())
        .unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

fn search_paths() -> Vec<PathBuf> {
    let process_path = std::env::var_os("PATH");
    let home = std::env::var_os("HOME").map(PathBuf::from);
    candidate_paths(process_path.as_deref(), login_shell_path(), home.as_deref())
}

/// Process `PATH` first, then the login shell's, then the fixed installer
/// directories; duplicates are dropped so the first occurrence wins.
pub(crate) fn candidate_paths(
    process_path: Option<&OsStr>,
    login_shell_path: Option<&OsStr>,
    home: Option<&Path>,
) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut push = |path: PathBuf| {
        if !path.as_os_str().is_empty() && !paths.contains(&path) {
            paths.push(path);
        }
    };
    for value in [process_path, login_shell_path].into_iter().flatten() {
        for entry in std::env::split_paths(value) {
            push(entry);
        }
    }
    if let Some(home) = home {
        for relative in [
            // Native installers for Claude Code and the Cursor CLI.
            ".local/bin",
            ".claude/local",
            ".cursor/bin",
            // The Grok Build installer's fixed bin directory.
            ".grok/bin",
            ".codex/bin",
            // Common user-level Node.js and Rust tool locations.
            ".npm-global/bin",
            ".volta/bin",
            ".cargo/bin",
            "bin",
        ] {
            push(home.join(relative));
        }
        for bin in version_manager_bins(home) {
            push(bin);
        }
    }
    for fixed in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/home/linuxbrew/.linuxbrew/bin",
        "/opt/local/bin",
        "/usr/bin",
        "/bin",
        "/snap/bin",
        "/var/lib/flatpak/exports/bin",
    ] {
        push(PathBuf::from(fixed));
    }
    paths
}

/// `nvm` and `fnm` keep one directory per Node.js version; newest first, so a
/// globally installed CLI resolves from the version the user upgraded to last.
fn version_manager_bins(home: &Path) -> Vec<PathBuf> {
    let mut bins = Vec::new();
    for (root, suffix) in [
        (home.join(".nvm/versions/node"), "bin"),
        (
            home.join(".local/share/fnm/node-versions"),
            "installation/bin",
        ),
        (
            home.join("Library/Application Support/fnm/node-versions"),
            "installation/bin",
        ),
    ] {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut versions: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        versions.sort();
        versions.reverse();
        bins.extend(versions.into_iter().map(|version| version.join(suffix)));
    }
    bins
}

pub(crate) fn resolve_program_from_paths(
    program: AllowedProgram,
    path_entries: &[PathBuf],
) -> Option<PathBuf> {
    path_entries
        .iter()
        .map(|directory| directory.join(program.executable()))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Runs a provider command in the user's terminal application and waits for the
/// script to report the command's exit status through a private marker file.
pub(crate) async fn run_visible(
    program: AllowedProgram,
    args: Vec<String>,
) -> Result<(), VisibleProcessError> {
    let launch = program_launch(program).ok_or(VisibleProcessError::NotInstalled)?;
    let search = search_paths();
    let terminal = terminal_launcher(&search).ok_or(VisibleProcessError::NoTerminal)?;
    let session =
        VisibleSession::create(&launch, &args).map_err(|_| VisibleProcessError::Failed)?;
    let result = session.run(&terminal).await;
    session.cleanup();
    result
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TerminalLauncher {
    executable: PathBuf,
    args_before_script: Vec<&'static str>,
}

/// Picks the first terminal application present on the machine. Each entry knows
/// how that terminal accepts a script to run.
pub(crate) fn terminal_launcher(path_entries: &[PathBuf]) -> Option<TerminalLauncher> {
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("open", &["-a", "Terminal"])]
    } else {
        &[
            ("gnome-terminal", &["--"]),
            ("konsole", &["-e"]),
            ("xfce4-terminal", &["-x"]),
            ("ptyxis", &["--"]),
            ("kitty", &[]),
            ("alacritty", &["-e"]),
            ("foot", &[]),
            ("x-terminal-emulator", &["-e"]),
            ("xterm", &["-e"]),
        ]
    };
    candidates.iter().find_map(|(name, args)| {
        path_entries
            .iter()
            .map(|directory| directory.join(name))
            .find(|candidate| is_executable_file(candidate))
            .map(|executable| TerminalLauncher {
                executable,
                args_before_script: args.to_vec(),
            })
    })
}

struct VisibleSession {
    directory: PathBuf,
    script: PathBuf,
    marker: PathBuf,
}

impl VisibleSession {
    fn create(launch: &ProgramLaunch, args: &[String]) -> std::io::Result<Self> {
        let unique = format!(
            "dashy-setup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        );
        let directory = std::env::temp_dir().join(unique);
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
        // Terminal.app runs `.command` files; Linux terminals take any script path.
        let script = directory.join(if cfg!(target_os = "macos") {
            "dashy-setup.command"
        } else {
            "dashy-setup.sh"
        });
        let marker = directory.join("status");
        let body = render_script(launch, args, &marker)
            .ok_or_else(|| std::io::Error::other("command is not representable in a shell"))?;
        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o700)
                .open(&script)?;
            file.write_all(body.as_bytes())?;
        }
        Ok(Self {
            directory,
            script,
            marker,
        })
    }

    async fn run(&self, terminal: &TerminalLauncher) -> Result<(), VisibleProcessError> {
        let mut child = tokio::process::Command::new(&terminal.executable)
            .args(&terminal.args_before_script)
            .arg(&self.script)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| VisibleProcessError::Failed)?;
        // Terminal launchers usually hand off to a running instance and exit at
        // once; reap them in the background and watch the marker instead.
        tokio::spawn(async move {
            let _ = child.wait().await;
        });

        let deadline = tokio::time::Instant::now() + VISIBLE_RUN_TIMEOUT;
        loop {
            if let Ok(contents) = std::fs::read_to_string(&self.marker) {
                return classify_marker(&contents);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(VisibleProcessError::Failed);
            }
            tokio::time::sleep(VISIBLE_RUN_POLL).await;
        }
    }

    fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn classify_marker(contents: &str) -> Result<(), VisibleProcessError> {
    match contents.trim().parse::<i32>() {
        Ok(0) => Ok(()),
        Ok(SHELL_COMMAND_NOT_FOUND) => Err(VisibleProcessError::NotInstalled),
        _ => Err(VisibleProcessError::Failed),
    }
}

/// The script writes the status to a temporary name and renames it so Dashy
/// never reads a half-written marker.
fn render_script(launch: &ProgramLaunch, args: &[String], marker: &Path) -> Option<String> {
    let mut command = vec![shell_quote(launch.executable.as_os_str())?];
    for arg in &launch.prefix_args {
        command.push(shell_quote(arg)?);
    }
    for arg in args {
        command.push(shell_quote(OsStr::new(arg))?);
    }
    let marker = shell_quote(marker.as_os_str())?;
    Some(format!(
        "#!/bin/sh\n\
         {command}\n\
         dashy_status=$?\n\
         printf '%s' \"$dashy_status\" > {marker}.tmp && mv -f {marker}.tmp {marker}\n\
         if [ \"$dashy_status\" -ne 0 ]; then\n\
         \x20 printf '\\nDashy: the command exited with status %s.\\n' \"$dashy_status\"\n\
         fi\n\
         printf '\\nDashy: you can close this window. Press Enter to close it now.\\n'\n\
         read -r dashy_unused 2>/dev/null\n\
         exit \"$dashy_status\"\n",
        command = command.join(" "),
    ))
}

fn shell_quote(value: &OsStr) -> Option<String> {
    let text = value.to_str()?;
    if text.bytes().any(|byte| byte == 0) {
        return None;
    }
    Some(format!("'{}'", text.replace('\'', "'\\''")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "dashy-unix-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_executable(path: &Path, executable: bool) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "#!/bin/sh\n").unwrap();
        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn process_path_comes_first_and_fixed_installer_directories_follow() {
        let home = temp_root("home");
        let process = std::env::join_paths([home.join("first"), home.join("second")]).unwrap();
        let login = std::env::join_paths([home.join("second"), home.join("third")]).unwrap();

        let paths = candidate_paths(Some(&process), Some(&login), Some(&home));

        assert_eq!(paths[0], home.join("first"));
        assert_eq!(paths[1], home.join("second"));
        assert_eq!(paths[2], home.join("third"));
        assert_eq!(
            paths
                .iter()
                .filter(|path| **path == home.join("second"))
                .count(),
            1
        );
        for expected in [
            home.join(".local/bin"),
            home.join(".grok/bin"),
            home.join(".npm-global/bin"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
        ] {
            assert!(
                paths.contains(&expected),
                "{} is missing",
                expected.display()
            );
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn node_version_manager_bins_are_listed_newest_first() {
        let home = temp_root("nvm");
        std::fs::create_dir_all(home.join(".nvm/versions/node/v20.11.0/bin")).unwrap();
        std::fs::create_dir_all(home.join(".nvm/versions/node/v22.4.1/bin")).unwrap();
        std::fs::create_dir_all(
            home.join(".local/share/fnm/node-versions/v21.0.0/installation/bin"),
        )
        .unwrap();

        let paths = candidate_paths(None, None, Some(&home));
        let newest = paths
            .iter()
            .position(|path| *path == home.join(".nvm/versions/node/v22.4.1/bin"))
            .unwrap();
        let older = paths
            .iter()
            .position(|path| *path == home.join(".nvm/versions/node/v20.11.0/bin"))
            .unwrap();
        assert!(newest < older);
        assert!(
            paths.contains(&home.join(".local/share/fnm/node-versions/v21.0.0/installation/bin"))
        );
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn only_executable_files_resolve_and_the_first_directory_wins() {
        let root = temp_root("resolve");
        let stale = root.join("stale");
        let fresh = root.join("fresh");
        write_executable(&stale.join("claude"), false);
        std::fs::create_dir_all(stale.join("codex")).unwrap();
        write_executable(&fresh.join("claude"), true);
        write_executable(&fresh.join("gh"), true);
        write_executable(&stale.join("gh"), true);

        let paths = vec![stale.clone(), fresh.clone()];
        assert_eq!(
            resolve_program_from_paths(AllowedProgram::Claude, &paths),
            Some(fresh.join("claude"))
        );
        assert_eq!(
            resolve_program_from_paths(AllowedProgram::Codex, &paths),
            None
        );
        assert_eq!(
            resolve_program_from_paths(AllowedProgram::Gh, &paths),
            Some(stale.join("gh"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_directory_added_after_startup_resolves_without_restarting_dashy() {
        let root = temp_root("late-install");
        let startup = root.join("startup");
        let installed = root.join("installed-later");
        std::fs::create_dir_all(&startup).unwrap();
        let process = std::env::join_paths([&startup]).unwrap();

        // The fixed system directories are part of every search, so a machine
        // with gh installed globally still resolves something; the point is that
        // the newly installed copy is not found until the login PATH carries it.
        let before = candidate_paths(Some(&process), None, None);
        assert_ne!(
            resolve_program_from_paths(AllowedProgram::Gh, &before),
            Some(installed.join("gh"))
        );

        write_executable(&installed.join("gh"), true);
        let login = std::env::join_paths([&installed]).unwrap();
        let after = candidate_paths(Some(&process), Some(&login), None);
        assert_eq!(
            resolve_program_from_paths(AllowedProgram::Gh, &after),
            Some(installed.join("gh"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_login_shell_path_is_read_with_a_bounded_wait() {
        let root = temp_root("shell");
        let shell = root.join("fake-shell");
        write_executable(&shell, true);
        std::fs::write(
            &shell,
            "#!/bin/sh\necho 'welcome banner'\nprintf '%s' \"/opt/tools/bin:/usr/bin\"\n",
        )
        .unwrap();
        assert_eq!(
            read_login_shell_path(&shell, Duration::from_secs(5)),
            Some(OsString::from("/opt/tools/bin:/usr/bin"))
        );

        let slow = root.join("slow-shell");
        write_executable(&slow, true);
        std::fs::write(&slow, "#!/bin/sh\nsleep 5\nprintf '%s' /never\n").unwrap();
        assert_eq!(
            read_login_shell_path(&slow, Duration::from_millis(100)),
            None
        );

        assert_eq!(
            read_login_shell_path(Path::new("relative-shell"), Duration::from_secs(1)),
            None
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shell_quoting_isolates_every_argument() {
        assert_eq!(shell_quote(OsStr::new("plain")), Some("'plain'".into()));
        assert_eq!(
            shell_quote(OsStr::new("it's $HOME `x`")),
            Some("'it'\\''s $HOME `x`'".into())
        );
        assert_eq!(shell_quote(OsStr::new("nul\0byte")), None);
    }

    #[test]
    fn the_setup_script_runs_the_resolved_executable_and_reports_its_status() {
        let launch = ProgramLaunch {
            executable: PathBuf::from("/opt/homebrew/bin/gh"),
            prefix_args: Vec::new(),
        };
        let script = render_script(
            &launch,
            &["auth".into(), "login".into(), "--web".into()],
            Path::new("/tmp/dashy-setup-1/status"),
        )
        .unwrap();

        assert!(script.starts_with("#!/bin/sh\n'/opt/homebrew/bin/gh' 'auth' 'login' '--web'\n"));
        assert!(script.contains("dashy_status=$?"));
        assert!(script.contains(
            "> '/tmp/dashy-setup-1/status'.tmp && mv -f '/tmp/dashy-setup-1/status'.tmp '/tmp/dashy-setup-1/status'"
        ));
        assert!(script.ends_with("exit \"$dashy_status\"\n"));
    }

    #[test]
    fn marker_status_maps_to_setup_outcomes() {
        assert_eq!(classify_marker("0\n"), Ok(()));
        assert_eq!(
            classify_marker("127"),
            Err(VisibleProcessError::NotInstalled)
        );
        assert_eq!(classify_marker("1"), Err(VisibleProcessError::Failed));
        assert_eq!(classify_marker("garbage"), Err(VisibleProcessError::Failed));
    }

    #[test]
    fn terminal_launchers_are_chosen_from_the_search_path_in_preference_order() {
        let root = temp_root("terminals");
        let bin = root.join("bin");
        let expected = if cfg!(target_os = "macos") {
            write_executable(&bin.join("open"), true);
            TerminalLauncher {
                executable: bin.join("open"),
                args_before_script: vec!["-a", "Terminal"],
            }
        } else {
            write_executable(&bin.join("xterm"), true);
            write_executable(&bin.join("konsole"), true);
            TerminalLauncher {
                executable: bin.join("konsole"),
                args_before_script: vec!["-e"],
            }
        };

        assert_eq!(
            terminal_launcher(std::slice::from_ref(&bin)),
            Some(expected)
        );
        assert_eq!(terminal_launcher(&[root.join("empty")]), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn a_visible_session_round_trips_the_status_through_the_marker() {
        let root = temp_root("session");
        let bin = root.join("bin");
        // A stand-in "terminal" that simply runs the script it is handed.
        let fake_terminal = bin.join("fake-terminal");
        write_executable(&fake_terminal, true);
        std::fs::write(
            &fake_terminal,
            "#!/bin/sh\nexec \"$@\" </dev/null >/dev/null 2>&1\n",
        )
        .unwrap();
        let tool = bin.join("tool");
        write_executable(&tool, true);
        std::fs::write(&tool, "#!/bin/sh\nexit 3\n").unwrap();

        let launch = ProgramLaunch {
            executable: tool,
            prefix_args: Vec::new(),
        };
        let session = VisibleSession::create(&launch, &["login".into()]).unwrap();
        let launcher = TerminalLauncher {
            executable: fake_terminal,
            args_before_script: Vec::new(),
        };
        let outcome = session.run(&launcher).await;
        session.cleanup();

        assert_eq!(outcome, Err(VisibleProcessError::Failed));
        assert!(!session.directory.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
