use async_trait::async_trait;
use std::{future::Future, path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    task::JoinHandle,
    time::{timeout_at, Instant as TokioInstant},
};

pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_CLEANUP_RESERVATION: Duration = Duration::from_millis(100);
const MIN_CLEANUP_RESERVATION: Duration = Duration::from_millis(1);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(all(windows, target_arch = "x86_64"))]
const CODEX_WINDOWS_PACKAGE: &str = "codex-win32-x64";
#[cfg(all(windows, target_arch = "x86_64"))]
const CODEX_WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";
#[cfg(all(windows, target_arch = "aarch64"))]
const CODEX_WINDOWS_PACKAGE: &str = "codex-win32-arm64";
#[cfg(all(windows, target_arch = "aarch64"))]
const CODEX_WINDOWS_TARGET: &str = "aarch64-pc-windows-msvc";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowedProgram {
    Gh,
    Codex,
    Claude,
    Grok,
    CursorAgent,
    Winget,
    Brew,
}

impl AllowedProgram {
    pub fn executable(self) -> &'static str {
        match self {
            Self::Gh => "gh",
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Grok => "grok",
            Self::CursorAgent => "cursor-agent",
            Self::Winget => "winget",
            Self::Brew => "brew",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProgramLaunch {
    pub(crate) executable: PathBuf,
    pub(crate) prefix_args: Vec<std::ffi::OsString>,
}

fn program_launch(program: AllowedProgram) -> ProgramLaunch {
    #[cfg(windows)]
    {
        let path_entries = current_windows_program_search_paths();
        if let Some(launch) = resolve_windows_program_from_paths(program, &path_entries) {
            return launch;
        }
    }
    #[cfg(unix)]
    {
        if let Some(launch) = super::unix::program_launch(program) {
            return launch;
        }
    }

    ProgramLaunch {
        executable: PathBuf::from(program.executable()),
        prefix_args: Vec::new(),
    }
}

#[cfg(windows)]
fn current_windows_program_search_paths() -> Vec<PathBuf> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let process_path = std::env::var_os("PATH");
    let user_path = read_windows_registry_string(HKEY_CURRENT_USER, "Environment", "Path");
    let machine_path = read_windows_registry_string(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        "Path",
    );
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
    let program_files_x86 = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from);
    let user_profile = std::env::var_os("USERPROFILE").map(PathBuf::from);

    windows_program_search_paths(
        process_path.as_deref(),
        user_path.as_deref(),
        machine_path.as_deref(),
        local_app_data.as_deref(),
        program_files.as_deref(),
        program_files_x86.as_deref(),
        user_profile.as_deref(),
    )
}

#[cfg(windows)]
fn windows_program_search_paths(
    process_path: Option<&std::ffi::OsStr>,
    user_path: Option<&std::ffi::OsStr>,
    machine_path: Option<&std::ffi::OsStr>,
    local_app_data: Option<&std::path::Path>,
    program_files: Option<&std::path::Path>,
    program_files_x86: Option<&std::path::Path>,
    user_profile: Option<&std::path::Path>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for value in [process_path, user_path, machine_path]
        .into_iter()
        .flatten()
    {
        paths.extend(std::env::split_paths(value));
    }
    if let Some(root) = local_app_data {
        paths.push(root.join("Microsoft/WinGet/Links"));
        // The Cursor CLI installer's fixed home; covers a wiped user PATH.
        paths.push(root.join("cursor-agent"));
    }
    if let Some(root) = program_files {
        paths.push(root.join("WinGet/Links"));
    }
    if let Some(root) = program_files_x86 {
        paths.push(root.join("WinGet/Links"));
    }
    if let Some(root) = user_profile {
        // The Grok CLI installer's fixed bin directory; covers a wiped user PATH.
        paths.push(root.join(".grok/bin"));
    }
    paths
}

#[cfg(windows)]
fn read_windows_registry_string(
    root: windows::Win32::System::Registry::HKEY,
    subkey: &str,
    value_name: &str,
) -> Option<std::ffi::OsString> {
    use std::os::windows::ffi::OsStringExt;
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::ERROR_MORE_DATA,
            System::Registry::{RegGetValueW, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ},
        },
    };

    let subkey = subkey
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let value_name = value_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ;
    let mut byte_count = 0_u32;
    // SAFETY: Both strings are NUL-terminated and remain alive for the call. The first query
    // requests only the required buffer size and does not pass a data pointer.
    let size_result = unsafe {
        RegGetValueW(
            root,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value_name.as_ptr()),
            flags,
            None,
            None,
            Some(&mut byte_count),
        )
    };
    if size_result.0 != 0 || byte_count == 0 {
        return None;
    }

    loop {
        let mut value = vec![0_u16; (byte_count as usize).div_ceil(2)];
        let mut available_bytes = byte_count;
        // SAFETY: The buffer is writable for `available_bytes`, the key/value pointers are valid
        // NUL-terminated strings, and RegGetValueW does not retain any supplied pointer.
        let read_result = unsafe {
            RegGetValueW(
                root,
                PCWSTR(subkey.as_ptr()),
                PCWSTR(value_name.as_ptr()),
                flags,
                None,
                Some(value.as_mut_ptr().cast()),
                Some(&mut available_bytes),
            )
        };
        if read_result == ERROR_MORE_DATA {
            byte_count = available_bytes;
            continue;
        }
        if read_result.0 != 0 {
            return None;
        }
        value.truncate((available_bytes as usize).div_ceil(2));
        while value.last() == Some(&0) {
            value.pop();
        }
        return Some(std::ffi::OsString::from_wide(&value));
    }
}

#[cfg(windows)]
fn resolve_windows_program_from_paths(
    program: AllowedProgram,
    path_entries: &[PathBuf],
) -> Option<ProgramLaunch> {
    if let Some(executable) = path_entries
        .iter()
        .map(|directory| directory.join(format!("{}.exe", program.executable())))
        .find(|candidate| candidate.is_file())
    {
        return Some(ProgramLaunch {
            executable,
            prefix_args: Vec::new(),
        });
    }

    // .cmd shims cannot be spawned as direct children, so shim-distributed programs
    // resolve to the real executable (or node plus script) behind their wrapper.
    match program {
        AllowedProgram::Codex => resolve_codex_npm_shim(path_entries),
        AllowedProgram::CursorAgent => resolve_cursor_agent_shim(path_entries),
        _ => None,
    }
}

#[cfg(windows)]
fn resolve_codex_npm_shim(path_entries: &[PathBuf]) -> Option<ProgramLaunch> {
    if let Some(executable) = path_entries.iter().find_map(|directory| {
        let shim = directory.join("codex.cmd");
        if !shim.is_file() {
            return None;
        }

        let package_root = directory.join("node_modules/@openai/codex");
        [
            package_root
                .join("node_modules/@openai")
                .join(CODEX_WINDOWS_PACKAGE)
                .join("vendor")
                .join(CODEX_WINDOWS_TARGET)
                .join("bin/codex.exe"),
            package_root
                .join("vendor")
                .join(CODEX_WINDOWS_TARGET)
                .join("bin/codex.exe"),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file())
    }) {
        return Some(ProgramLaunch {
            executable,
            prefix_args: Vec::new(),
        });
    }

    // Keep a compatibility fallback for future npm layouts. Current official
    // packages are resolved to their bundled executable above, which preserves
    // direct child-process ownership for timeout cleanup.
    let node = path_entries
        .iter()
        .map(|directory| directory.join("node.exe"))
        .find(|candidate| candidate.is_file())?;
    let script = path_entries.iter().find_map(|directory| {
        let shim = directory.join("codex.cmd");
        let script = directory.join("node_modules/@openai/codex/bin/codex.js");
        (shim.is_file() && script.is_file()).then_some(script)
    })?;

    Some(ProgramLaunch {
        executable: node,
        prefix_args: vec![script.into_os_string()],
    })
}

// The Cursor CLI ships no executable: cursor-agent.cmd wraps node.exe plus a
// versioned index.js under <shim dir>/versions/<version>/. Version directories are
// date-stamped (for example 2026.08.31-4057e58), so the lexicographically greatest
// name is the newest payload; a partially downloaded update leaves no index.js and
// therefore cannot win.
#[cfg(windows)]
fn resolve_cursor_agent_shim(path_entries: &[PathBuf]) -> Option<ProgramLaunch> {
    path_entries.iter().find_map(|directory| {
        let shim = directory.join("cursor-agent.cmd");
        if !shim.is_file() {
            return None;
        }

        let mut newest: Option<(std::ffi::OsString, PathBuf)> = None;
        for entry in std::fs::read_dir(directory.join("versions"))
            .ok()?
            .flatten()
        {
            let version_dir = entry.path();
            if !version_dir.join("index.js").is_file() {
                continue;
            }
            let name = entry.file_name();
            if newest
                .as_ref()
                .is_none_or(|(newest_name, _)| name > *newest_name)
            {
                newest = Some((name, version_dir));
            }
        }
        let (_, version_dir) = newest?;

        let node = [version_dir.join("node.exe"), directory.join("node.exe")]
            .into_iter()
            .find(|candidate| candidate.is_file())
            .or_else(|| {
                path_entries
                    .iter()
                    .map(|entry| entry.join("node.exe"))
                    .find(|candidate| candidate.is_file())
            })?;

        Some(ProgramLaunch {
            executable: node,
            prefix_args: vec![version_dir.join("index.js").into_os_string()],
        })
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessError {
    NotInstalled,
    Timeout,
    NonZero(i32),
    OutputLimit,
    JsonRpc { code: i64, message: String },
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibleProcessError {
    NotInstalled,
    UnsupportedPlatform,
    NoTerminal,
    Failed,
}

#[async_trait]
pub trait VisibleRunner: Send + Sync {
    async fn run_visible(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
    ) -> Result<(), VisibleProcessError>;
}

#[async_trait]
pub trait CaptureRunner: Send + Sync {
    async fn capture(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CapturedOutput, ProcessError>;
}

#[async_trait]
pub trait JsonRpcRunner: Send + Sync {
    async fn request(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
        requests: Vec<serde_json::Value>,
        response_id: u64,
        timeout: Duration,
    ) -> Result<serde_json::Value, ProcessError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemProcessRunner;

#[async_trait]
impl VisibleRunner for SystemProcessRunner {
    async fn run_visible(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
    ) -> Result<(), VisibleProcessError> {
        #[cfg(windows)]
        {
            const CREATE_NEW_CONSOLE: u32 = 0x00000010;
            let launch = program_launch(program);
            let status = Command::new(&launch.executable)
                .args(&launch.prefix_args)
                .args(args)
                .stdin(std::process::Stdio::inherit())
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .creation_flags(CREATE_NEW_CONSOLE)
                .status()
                .await
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        VisibleProcessError::NotInstalled
                    } else {
                        VisibleProcessError::Failed
                    }
                })?;
            return status
                .success()
                .then_some(())
                .ok_or(VisibleProcessError::Failed);
        }
        #[cfg(unix)]
        {
            super::unix::run_visible(program, args).await
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = (program, args);
            Err(VisibleProcessError::UnsupportedPlatform)
        }
    }
}

struct BoundedReadTask {
    handle: Option<JoinHandle<Result<Vec<u8>, ProcessError>>>,
    completed: Option<Result<Vec<u8>, ProcessError>>,
}

impl BoundedReadTask {
    fn spawn<R>(reader: R) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        Self {
            handle: Some(tokio::spawn(read_bounded(reader))),
            completed: None,
        }
    }

    async fn wait_for_json_response<F>(
        &mut self,
        response: F,
    ) -> Result<serde_json::Value, ProcessError>
    where
        F: Future<Output = Result<serde_json::Value, ProcessError>>,
    {
        tokio::pin!(response);
        let handle = self.handle.as_mut().ok_or(ProcessError::Io)?;
        let stderr_result = tokio::select! {
            response_result = &mut response => return response_result,
            stderr_result = handle => stderr_result,
        };
        let stderr_result = stderr_result
            .map_err(|_| ProcessError::Io)
            .and_then(|result| result);
        let stderr_error = stderr_result.as_ref().err().cloned();
        self.completed = Some(stderr_result);
        self.handle = None;

        match stderr_error {
            Some(error) => Err(error),
            None => response.await,
        }
    }

    async fn finish(&mut self, deadline: TokioInstant) -> Result<Vec<u8>, ProcessError> {
        if let Some(result) = self.completed.take() {
            return result;
        }

        let mut handle = self.handle.take().ok_or(ProcessError::Io)?;
        match timeout_at(deadline, &mut handle).await {
            Ok(result) => result.map_err(|_| ProcessError::Io)?,
            Err(_) => {
                handle.abort();
                Err(ProcessError::Timeout)
            }
        }
    }
}

#[async_trait]
impl CaptureRunner for SystemProcessRunner {
    async fn capture(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
        timeout_duration: Duration,
    ) -> Result<CapturedOutput, ProcessError> {
        let (operation_deadline, deadline) = timeout_deadlines(timeout_duration);
        let mut child = spawn_piped(program, args, false)?;
        let stdout = child.stdout.take().ok_or(ProcessError::Io)?;
        let stderr = child.stderr.take().ok_or(ProcessError::Io)?;

        let outcome = timeout_at(operation_deadline, async {
            tokio::try_join!(
                async { child.wait().await.map_err(map_spawn_error) },
                async { tokio::try_join!(read_bounded(stdout), read_bounded(stderr)) }
            )
        })
        .await;

        match outcome {
            Ok(Ok((status, (stdout, stderr)))) => {
                let exit_code = status.code().unwrap_or(-1);
                if status.success() {
                    Ok(CapturedOutput {
                        stdout: bounded_text(stdout)?,
                        stderr: bounded_text(stderr)?,
                        exit_code,
                    })
                } else {
                    Err(ProcessError::NonZero(exit_code))
                }
            }
            Ok(Err(error)) => match terminate_tokio_before(&mut child, deadline).await {
                Ok(()) => Err(error),
                Err(ProcessError::Timeout) => Err(ProcessError::Timeout),
                Err(cleanup_error) => Err(cleanup_error),
            },
            Err(_) => {
                let _ = terminate_tokio_before(&mut child, deadline).await;
                Err(ProcessError::Timeout)
            }
        }
    }
}

#[async_trait]
impl JsonRpcRunner for SystemProcessRunner {
    async fn request(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
        requests: Vec<serde_json::Value>,
        response_id: u64,
        timeout_duration: Duration,
    ) -> Result<serde_json::Value, ProcessError> {
        let (operation_deadline, deadline) = timeout_deadlines(timeout_duration);
        let mut child = spawn_piped(program, args, true)?;
        let mut stdin = child.stdin.take().ok_or(ProcessError::Io)?;
        let stdout = child.stdout.take().ok_or(ProcessError::Io)?;
        let stderr = child.stderr.take().ok_or(ProcessError::Io)?;
        let mut stderr_task = BoundedReadTask::spawn(stderr);
        let response = read_json_response(stdout, response_id);

        let request_result = timeout_at(operation_deadline, async {
            for request in requests {
                let mut line = serde_json::to_vec(&request).map_err(|_| ProcessError::Io)?;
                line.push(b'\n');
                stdin.write_all(&line).await.map_err(map_spawn_error)?;
            }

            stderr_task.wait_for_json_response(response).await
        })
        .await;

        // JSON-RPC hosts commonly stay alive after responding. Always tear the
        // host down and reap it before returning, including parse and timeout paths.
        let termination_result = terminate_tokio_before(&mut child, deadline).await;
        let stderr_result = stderr_task.finish(deadline).await;
        drop(stdin);

        termination_result?;
        let _stderr = stderr_result?;

        match request_result {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(ProcessError::Timeout),
        }
    }
}

fn spawn_piped(
    program: AllowedProgram,
    args: Vec<String>,
    with_stdin: bool,
) -> Result<Child, ProcessError> {
    let launch = program_launch(program);
    let mut command = Command::new(&launch.executable);
    command
        .args(&launch.prefix_args)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if with_stdin {
        command.stdin(std::process::Stdio::piped());
    } else {
        command.stdin(std::process::Stdio::null());
    }
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    // Desktop-launched processes on macOS and Linux inherit a minimal PATH; the
    // provider CLIs spawn their own helpers (node, gh), so hand them the same
    // search path Dashy resolved the CLI from.
    #[cfg(unix)]
    command.env("PATH", super::unix::child_path_value());
    command.kill_on_drop(true).spawn().map_err(map_spawn_error)
}

async fn read_bounded<R: AsyncRead + Unpin>(mut reader: R) -> Result<Vec<u8>, ProcessError> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let count = reader.read(&mut buffer).await.map_err(map_spawn_error)?;
        if count == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(count) > MAX_OUTPUT_BYTES {
            return Err(ProcessError::OutputLimit);
        }
        output.extend_from_slice(&buffer[..count]);
    }
}

async fn read_json_response<R: AsyncRead + Unpin>(
    mut reader: R,
    response_id: u64,
) -> Result<serde_json::Value, ProcessError> {
    let mut total = 0_usize;
    let mut line = Vec::new();
    let mut buffer = [0_u8; 4096];

    loop {
        let count = reader.read(&mut buffer).await.map_err(map_spawn_error)?;
        if count == 0 {
            return Err(ProcessError::Io);
        }
        for byte in &buffer[..count] {
            total = total.saturating_add(1);
            if total > MAX_OUTPUT_BYTES || line.len() >= MAX_OUTPUT_BYTES {
                return Err(ProcessError::OutputLimit);
            }
            if *byte == b'\n' {
                let value: serde_json::Value =
                    serde_json::from_slice(&line).map_err(|_| ProcessError::Io)?;
                line.clear();
                if let Some(result) = classify_json_response(&value, response_id) {
                    return result;
                }
            } else if *byte != b'\r' {
                line.push(*byte);
            }
        }
    }
}

fn classify_json_response(
    value: &serde_json::Value,
    response_id: u64,
) -> Option<Result<serde_json::Value, ProcessError>> {
    if value.get("id").and_then(serde_json::Value::as_u64) != Some(response_id) {
        return None;
    }
    if let Some(result) = value.get("result") {
        return Some(Ok(result.clone()));
    }

    let outcome = match value.get("error").map(|error| {
        (
            error.get("code").and_then(serde_json::Value::as_i64),
            error.get("message").and_then(serde_json::Value::as_str),
        )
    }) {
        Some((Some(code), Some(message))) => ProcessError::JsonRpc {
            code,
            message: message.to_owned(),
        },
        _ => ProcessError::Io,
    };
    Some(Err(outcome))
}

async fn terminate_tokio_before(
    child: &mut Child,
    deadline: TokioInstant,
) -> Result<(), ProcessError> {
    // Start termination without awaiting it, then use the caller's absolute
    // deadline to reap the direct child even if kill races with natural exit.
    let _ = child.start_kill();
    match timeout_at(deadline, child.wait()).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(map_spawn_error(error)),
        Err(_) => Err(ProcessError::Timeout),
    }
}

fn timeout_deadlines(total: Duration) -> (TokioInstant, TokioInstant) {
    let now = TokioInstant::now();
    let (operation, _) = split_timeout_budget(total);
    (now + operation, now + total)
}

fn split_timeout_budget(total: Duration) -> (Duration, Duration) {
    if total.is_zero() {
        return (Duration::ZERO, Duration::ZERO);
    }

    // Reserve at least 1 ms when available, about a quarter for short
    // timeouts, and no more than 100 ms for normal request budgets.
    let cleanup = (total / 4)
        .max(MIN_CLEANUP_RESERVATION)
        .min(MAX_CLEANUP_RESERVATION)
        .min(total);
    (total.saturating_sub(cleanup), cleanup)
}

fn bounded_text(bytes: Vec<u8>) -> Result<String, ProcessError> {
    if bytes.len() > MAX_OUTPUT_BYTES {
        return Err(ProcessError::OutputLimit);
    }
    String::from_utf8(bytes).map_err(|_| ProcessError::Io)
}

fn map_spawn_error(error: std::io::Error) -> ProcessError {
    if error.kind() == std::io::ErrorKind::NotFound {
        ProcessError::NotInstalled
    } else {
        ProcessError::Io
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_json_rpc_result_is_returned() {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": { "authenticated": true }
        });

        assert_eq!(
            classify_json_response(&response, 7),
            Some(Ok(serde_json::json!({ "authenticated": true })))
        );
    }

    #[test]
    fn matching_json_rpc_error_preserves_code_and_message() {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "error": {
                "code": -32001,
                "message": "Authentication required"
            }
        });

        assert_eq!(
            classify_json_response(&response, 7),
            Some(Err(ProcessError::JsonRpc {
                code: -32001,
                message: "Authentication required".to_owned(),
            }))
        );
    }

    #[test]
    fn matching_json_rpc_response_with_malformed_error_is_rejected() {
        let malformed_responses = [
            serde_json::json!({ "jsonrpc": "2.0", "id": 7 }),
            serde_json::json!({ "jsonrpc": "2.0", "id": 7, "error": null }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 7,
                "error": { "code": "-32001", "message": "Authentication required" }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 7,
                "error": { "code": -32001, "message": 401 }
            }),
        ];

        for response in malformed_responses {
            assert_eq!(
                classify_json_response(&response, 7),
                Some(Err(ProcessError::Io))
            );
        }
    }

    #[test]
    fn unrelated_json_rpc_responses_and_notifications_are_ignored() {
        let unrelated_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 8,
            "result": { "authenticated": true }
        });
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "account/updated",
            "params": { "authenticated": true }
        });

        assert_eq!(classify_json_response(&unrelated_response, 7), None);
        assert_eq!(classify_json_response(&notification, 7), None);
    }

    #[test]
    fn executable_names_are_fixed() {
        assert_eq!(AllowedProgram::Gh.executable(), "gh");
        assert_eq!(AllowedProgram::Codex.executable(), "codex");
        assert_eq!(AllowedProgram::Claude.executable(), "claude");
        assert_eq!(AllowedProgram::Grok.executable(), "grok");
        assert_eq!(AllowedProgram::CursorAgent.executable(), "cursor-agent");
        assert_eq!(AllowedProgram::Winget.executable(), "winget");
        assert_eq!(AllowedProgram::Brew.executable(), "brew");
    }

    #[cfg(windows)]
    #[test]
    fn npm_codex_shim_resolves_to_the_bundled_native_executable() {
        let unique = format!(
            "dashy-codex-resolver-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let npm = root.join("npm");
        let executable = npm
            .join("node_modules/@openai/codex/node_modules/@openai")
            .join(CODEX_WINDOWS_PACKAGE)
            .join("vendor")
            .join(CODEX_WINDOWS_TARGET)
            .join("bin/codex.exe");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(npm.join("codex.cmd"), "npm shim").unwrap();
        std::fs::write(&executable, "codex executable").unwrap();

        let launch =
            resolve_windows_program_from_paths(AllowedProgram::Codex, std::slice::from_ref(&npm))
                .unwrap();

        assert_eq!(launch.executable, executable);
        assert!(launch.prefix_args.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn provider_directory_added_after_startup_resolves_without_restarting_dashy() {
        let unique = format!(
            "dashy-provider-path-refresh-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let startup_path = root.join("startup-path");
        let installed_path = root.join("installed-after-startup");
        std::fs::create_dir_all(&startup_path).unwrap();
        std::fs::create_dir_all(&installed_path).unwrap();
        let startup_path_value = std::env::join_paths([&startup_path]).unwrap();

        let before = windows_program_search_paths(
            Some(&startup_path_value),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            resolve_windows_program_from_paths(AllowedProgram::Gh, &before),
            None
        );

        let executable = installed_path.join("gh.exe");
        std::fs::write(&executable, "installed executable").unwrap();
        let refreshed_user_path = std::env::join_paths([&installed_path]).unwrap();
        let after = windows_program_search_paths(
            Some(&startup_path_value),
            Some(&refreshed_user_path),
            None,
            None,
            None,
            None,
            None,
        );
        let launch = resolve_windows_program_from_paths(AllowedProgram::Gh, &after).unwrap();

        assert_eq!(launch.executable, executable);
        assert!(launch.prefix_args.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn fixed_winget_links_are_searched_even_when_process_path_is_stale() {
        let unique = format!(
            "dashy-winget-links-refresh-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let local_app_data = root.join("local-app-data");
        let links = local_app_data.join("Microsoft/WinGet/Links");
        let executable = links.join("claude.exe");
        std::fs::create_dir_all(&links).unwrap();
        std::fs::write(&executable, "installed executable").unwrap();

        let paths =
            windows_program_search_paths(None, None, None, Some(&local_app_data), None, None, None);
        let launch = resolve_windows_program_from_paths(AllowedProgram::Claude, &paths).unwrap();

        assert_eq!(launch.executable, executable);
        assert!(launch.prefix_args.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn fixed_grok_and_cursor_homes_are_searched_even_when_user_path_is_wiped() {
        let unique = format!(
            "dashy-fixed-provider-homes-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let user_profile = root.join("user-profile");
        let grok_bin = user_profile.join(".grok/bin");
        let grok = grok_bin.join("grok.exe");
        std::fs::create_dir_all(&grok_bin).unwrap();
        std::fs::write(&grok, "grok executable").unwrap();

        let local_app_data = root.join("local-app-data");
        let cursor_home = local_app_data.join("cursor-agent");
        let cursor_version = cursor_home.join("versions/2026.08.31-4057e58");
        std::fs::create_dir_all(&cursor_version).unwrap();
        std::fs::write(cursor_home.join("cursor-agent.cmd"), "wrapper").unwrap();
        std::fs::write(cursor_version.join("index.js"), "payload").unwrap();
        std::fs::write(cursor_version.join("node.exe"), "bundled node").unwrap();

        let paths = windows_program_search_paths(
            None,
            None,
            None,
            Some(&local_app_data),
            None,
            None,
            Some(&user_profile),
        );

        let grok_launch = resolve_windows_program_from_paths(AllowedProgram::Grok, &paths).unwrap();
        assert_eq!(grok_launch.executable, grok);
        assert!(grok_launch.prefix_args.is_empty());

        let cursor_launch =
            resolve_windows_program_from_paths(AllowedProgram::CursorAgent, &paths).unwrap();
        assert_eq!(cursor_launch.executable, cursor_version.join("node.exe"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn cursor_agent_cmd_shim_resolves_to_bundled_node_and_newest_versioned_script() {
        let unique = format!(
            "dashy-cursor-resolver-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let home = root.join("cursor-agent");
        let stale = home.join("versions/2026.01.01-aaaaaaa");
        let current = home.join("versions/2026.08.31-4057e58");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(home.join("cursor-agent.cmd"), "wrapper").unwrap();
        std::fs::write(stale.join("index.js"), "stale payload").unwrap();
        std::fs::write(current.join("index.js"), "current payload").unwrap();
        std::fs::write(current.join("node.exe"), "bundled node").unwrap();

        let launch = resolve_windows_program_from_paths(
            AllowedProgram::CursorAgent,
            std::slice::from_ref(&home),
        )
        .unwrap();

        assert_eq!(launch.executable, current.join("node.exe"));
        assert_eq!(launch.prefix_args.len(), 1);
        // Compare as paths: the fixture literal mixes separators, the resolver's
        // read_dir output does not, and only component equality matters.
        assert_eq!(
            PathBuf::from(&launch.prefix_args[0]),
            current.join("index.js")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn cursor_agent_shim_without_payload_or_node_does_not_resolve() {
        let unique = format!(
            "dashy-cursor-no-payload-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let home = root.join("cursor-agent");
        std::fs::create_dir_all(home.join("versions")).unwrap();
        std::fs::write(home.join("cursor-agent.cmd"), "wrapper").unwrap();
        // A stalled install: wrapper exists but versions/ holds no payload directory.
        std::fs::write(home.join("versions/partial-download.zip"), "zip").unwrap();

        assert_eq!(
            resolve_windows_program_from_paths(
                AllowedProgram::CursorAgent,
                std::slice::from_ref(&home),
            ),
            None
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_output_is_rejected() {
        let bytes = vec![b'x'; MAX_OUTPUT_BYTES + 1];
        assert_eq!(bounded_text(bytes).unwrap_err(), ProcessError::OutputLimit);
    }

    #[tokio::test]
    async fn oversized_json_rpc_stderr_remains_a_recoverable_error_after_it_is_consumed() {
        let stderr = tokio::io::repeat(b'x').take((MAX_OUTPUT_BYTES + 1) as u64);
        let mut stderr_task = BoundedReadTask::spawn(stderr);

        let response_result = stderr_task
            .wait_for_json_response(std::future::pending::<
                Result<serde_json::Value, ProcessError>,
            >())
            .await;
        let cleanup_result = stderr_task
            .finish(TokioInstant::now() + Duration::from_secs(1))
            .await;

        assert_eq!(response_result, Err(ProcessError::OutputLimit));
        assert_eq!(cleanup_result, Err(ProcessError::OutputLimit));
    }

    #[test]
    fn not_found_maps_to_not_installed() {
        let error = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        assert_eq!(map_spawn_error(error), ProcessError::NotInstalled);
    }

    #[test]
    fn timeout_budget_reserves_cleanup_inside_the_total_timeout() {
        let total = Duration::from_millis(500);
        let (operation, cleanup) = split_timeout_budget(total);

        assert_eq!(cleanup, Duration::from_millis(100));
        assert_eq!(operation + cleanup, total);
        assert!(cleanup < total);
    }

    #[test]
    fn short_timeout_keeps_a_nonzero_cleanup_reservation() {
        let total = Duration::from_millis(2);
        let (operation, cleanup) = split_timeout_budget(total);

        assert_eq!(cleanup, Duration::from_millis(1));
        assert_eq!(operation, Duration::from_millis(1));
        assert_eq!(operation + cleanup, total);
    }

    #[test]
    fn zero_timeout_does_not_extend_the_caller_deadline() {
        assert_eq!(
            split_timeout_budget(Duration::ZERO),
            (Duration::ZERO, Duration::ZERO)
        );
    }
}
