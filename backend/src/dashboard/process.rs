use async_trait::async_trait;
#[cfg(not(windows))]
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::{
    future::Future,
    io::{Read, Write},
    path::PathBuf,
    sync::mpsc::{self, TryRecvError},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    task::JoinHandle,
    time::{timeout_at, Instant as TokioInstant},
};

pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_CLEANUP_RESERVATION: Duration = Duration::from_millis(100);
const MIN_CLEANUP_RESERVATION: Duration = Duration::from_millis(1);
const PTY_CLEANUP_POLL_INTERVAL: Duration = Duration::from_millis(5);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(windows)]
const WINDOWS_RENDERED_SCREEN_SETTLE: Duration = Duration::from_secs(1);
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
    Winget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionMarker {
    Exact(String),
    ExactAlternative { first: String, second: String },
    Prefix(String),
}

impl CompletionMarker {
    fn is_valid(&self) -> bool {
        match self {
            Self::Exact(value) | Self::Prefix(value) => !value.is_empty(),
            Self::ExactAlternative { first, second } => !first.is_empty() && !second.is_empty(),
        }
    }

    fn matches(&self, line: &str) -> bool {
        match self {
            Self::Exact(value) => line == value,
            Self::ExactAlternative { first, second } => line == first || line == second,
            Self::Prefix(value) => line.starts_with(value),
        }
    }
}

impl AllowedProgram {
    pub fn executable(self) -> &'static str {
        match self {
            Self::Gh => "gh",
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Winget => "winget",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProgramLaunch {
    executable: PathBuf,
    prefix_args: Vec<std::ffi::OsString>,
}

fn program_launch(program: AllowedProgram) -> ProgramLaunch {
    #[cfg(windows)]
    {
        let path_entries = current_windows_program_search_paths();
        if let Some(launch) = resolve_windows_program_from_paths(program, &path_entries) {
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

    windows_program_search_paths(
        process_path.as_deref(),
        user_path.as_deref(),
        machine_path.as_deref(),
        local_app_data.as_deref(),
        program_files.as_deref(),
        program_files_x86.as_deref(),
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
    }
    if let Some(root) = program_files {
        paths.push(root.join("WinGet/Links"));
    }
    if let Some(root) = program_files_x86 {
        paths.push(root.join("WinGet/Links"));
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

    if program != AllowedProgram::Codex {
        return None;
    }

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

#[async_trait]
pub trait InteractiveRunner: Send + Sync {
    async fn run_command(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
        input: String,
        completion_markers: Vec<CompletionMarker>,
        exit_input: Option<String>,
        timeout: Duration,
    ) -> Result<String, ProcessError>;
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
        #[cfg(not(windows))]
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

#[async_trait]
impl InteractiveRunner for SystemProcessRunner {
    async fn run_command(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
        input: String,
        completion_markers: Vec<CompletionMarker>,
        exit_input: Option<String>,
        timeout_duration: Duration,
    ) -> Result<String, ProcessError> {
        tokio::task::spawn_blocking(move || {
            run_pty_command(
                program,
                args,
                input,
                completion_markers,
                exit_input,
                timeout_duration,
            )
        })
        .await
        .map_err(|_| ProcessError::Io)?
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InteractivePtyBackend {
    #[cfg(windows)]
    Conpty,
    #[cfg(not(windows))]
    PortablePty,
}

fn interactive_pty_backend() -> InteractivePtyBackend {
    #[cfg(windows)]
    {
        InteractivePtyBackend::Conpty
    }
    #[cfg(not(windows))]
    {
        InteractivePtyBackend::PortablePty
    }
}

#[derive(Debug, Clone, Copy)]
struct PtyExitStatus {
    success: bool,
    exit_code: i32,
}

trait PtyChild: Send {
    fn try_wait(&mut self) -> Result<Option<PtyExitStatus>, ProcessError>;
    fn request_termination(&mut self) -> Result<(), ProcessError>;
}

fn terminate_pty_before(child: &mut dyn PtyChild, deadline: Instant) -> Result<(), ProcessError> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    child.request_termination()?;
    loop {
        match child.try_wait()? {
            Some(_) => return Ok(()),
            None => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(ProcessError::Timeout);
                }
                std::thread::sleep(remaining.min(PTY_CLEANUP_POLL_INTERVAL));
            }
        }
    }
}

fn pty_failure_after_cleanup<T>(
    child: &mut dyn PtyChild,
    cleanup_deadline: Instant,
    error: ProcessError,
) -> Result<T, ProcessError> {
    terminate_pty_before(child, cleanup_deadline)?;
    Err(error)
}

struct PtySession {
    child: Box<dyn PtyChild>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    output_mode: PtyOutputMode,
}

#[derive(Debug, Clone, Copy)]
enum PtyOutputMode {
    #[cfg(not(windows))]
    LineStream,
    #[cfg(windows)]
    RenderedScreen,
}

#[cfg(not(windows))]
struct PortablePtyChild(Box<dyn portable_pty::Child + Send>);

#[cfg(not(windows))]
impl PtyChild for PortablePtyChild {
    fn try_wait(&mut self) -> Result<Option<PtyExitStatus>, ProcessError> {
        self.0
            .try_wait()
            .map(|status| {
                status.map(|status| PtyExitStatus {
                    success: status.success(),
                    exit_code: status.exit_code() as i32,
                })
            })
            .map_err(|_| ProcessError::Io)
    }

    fn request_termination(&mut self) -> Result<(), ProcessError> {
        self.0.kill().map_err(|_| ProcessError::Io)
    }
}

#[cfg(windows)]
struct ConptyChild(conpty::Process);

#[cfg(windows)]
impl PtyChild for ConptyChild {
    fn try_wait(&mut self) -> Result<Option<PtyExitStatus>, ProcessError> {
        if self.0.is_alive() {
            return Ok(None);
        }

        self.0
            .wait(Some(0))
            .map(|exit_code| {
                Some(PtyExitStatus {
                    success: exit_code == 0,
                    exit_code: exit_code as i32,
                })
            })
            .map_err(|_| ProcessError::Io)
    }

    fn request_termination(&mut self) -> Result<(), ProcessError> {
        self.0.exit(1).map_err(|_| ProcessError::Io)
    }
}

#[cfg(windows)]
struct ConptyWriter(conpty::io::PipeWriter);

#[cfg(windows)]
impl Write for ConptyWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // ConPTY delivers WriteFile input immediately. Calling FlushFileBuffers
        // here can wait for the terminal client and defeat the interaction deadline.
        Ok(())
    }
}

fn spawn_pty_session(
    program: AllowedProgram,
    args: Vec<String>,
    cleanup_deadline: Instant,
) -> Result<PtySession, ProcessError> {
    match interactive_pty_backend() {
        #[cfg(windows)]
        InteractivePtyBackend::Conpty => spawn_conpty_session(program, args, cleanup_deadline),
        #[cfg(not(windows))]
        InteractivePtyBackend::PortablePty => {
            spawn_portable_pty_session(program, args, cleanup_deadline)
        }
    }
}

fn initial_input_settle(output_mode: PtyOutputMode) -> Duration {
    #[cfg(windows)]
    {
        match output_mode {
            PtyOutputMode::RenderedScreen => WINDOWS_RENDERED_SCREEN_SETTLE,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = output_mode;
        Duration::ZERO
    }
}

#[cfg(windows)]
fn spawn_conpty_session(
    program: AllowedProgram,
    args: Vec<String>,
    cleanup_deadline: Instant,
) -> Result<PtySession, ProcessError> {
    let launch = program_launch(program);
    let mut command = std::process::Command::new(&launch.executable);
    command.args(&launch.prefix_args).args(args);
    let mut options = conpty::ProcessOptions::default();
    options.set_console_size(Some((80, 30)));
    let mut process = options
        .spawn(command)
        .map_err(|error| map_spawn_error(std::io::Error::from(error)))?;
    let reader = match process.output() {
        Ok(reader) => reader,
        Err(_) => {
            let mut child = ConptyChild(process);
            return pty_failure_after_cleanup(&mut child, cleanup_deadline, ProcessError::Io);
        }
    };
    let writer = match process.input() {
        Ok(writer) => writer,
        Err(_) => {
            let mut child = ConptyChild(process);
            return pty_failure_after_cleanup(&mut child, cleanup_deadline, ProcessError::Io);
        }
    };

    Ok(PtySession {
        child: Box::new(ConptyChild(process)),
        reader: Box::new(reader),
        writer: Box::new(ConptyWriter(writer)),
        output_mode: PtyOutputMode::RenderedScreen,
    })
}

#[cfg(not(windows))]
fn spawn_portable_pty_session(
    program: AllowedProgram,
    args: Vec<String>,
    cleanup_deadline: Instant,
) -> Result<PtySession, ProcessError> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 30,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|_| ProcessError::Io)?;

    let mut command = CommandBuilder::new(program.executable());
    command.args(args);
    let child = pair.slave.spawn_command(command).map_err(|error| {
        error
            .downcast_ref::<std::io::Error>()
            .map_or(ProcessError::Io, |io_error| {
                map_spawn_error(std::io::Error::new(io_error.kind(), "pty"))
            })
    })?;
    drop(pair.slave);
    let reader = match pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(_) => {
            let mut child = PortablePtyChild(child);
            return pty_failure_after_cleanup(&mut child, cleanup_deadline, ProcessError::Io);
        }
    };
    let writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(_) => {
            let mut child = PortablePtyChild(child);
            return pty_failure_after_cleanup(&mut child, cleanup_deadline, ProcessError::Io);
        }
    };

    Ok(PtySession {
        child: Box::new(PortablePtyChild(child)),
        reader,
        writer,
        output_mode: PtyOutputMode::LineStream,
    })
}

fn run_pty_command(
    program: AllowedProgram,
    args: Vec<String>,
    input: String,
    completion_markers: Vec<CompletionMarker>,
    exit_input: Option<String>,
    timeout_duration: Duration,
) -> Result<String, ProcessError> {
    let started = Instant::now();
    let (operation_budget, _) = split_timeout_budget(timeout_duration);
    let operation_deadline = started + operation_budget;
    let cleanup_deadline = started + timeout_duration;
    let PtySession {
        mut child,
        reader,
        writer,
        output_mode,
    } = spawn_pty_session(program, args, cleanup_deadline)?;

    let (output_sender, output_receiver) = mpsc::channel();
    let (readiness_sender, readiness_receiver) = mpsc::channel();
    let echoed_input = input.clone();
    std::thread::spawn(move || {
        let result = read_pty_output_with_readiness(
            reader,
            echoed_input,
            completion_markers,
            output_mode,
            readiness_sender,
        );
        let _ = output_sender.send(result);
    });

    let (initial_command_sender, initial_command_receiver) = mpsc::channel::<String>();
    let (writer_command_sender, writer_command_receiver) = mpsc::channel::<Option<String>>();
    let (initial_write_sender, initial_write_receiver) = mpsc::channel();
    let (exit_write_sender, exit_write_receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut writer = writer;
        let initial_result = initial_command_receiver
            .recv()
            .map_err(|_| ProcessError::Io)
            .and_then(|input| write_pty_input(&mut writer, &input));
        let initial_succeeded = initial_result.is_ok();
        let _ = initial_write_sender.send(initial_result);

        if initial_succeeded {
            let exit_result = match writer_command_receiver.recv() {
                Ok(exit_input) => {
                    write_exit_input_if_complete(&mut writer, true, exit_input.as_deref())
                        .map(|_| ())
                }
                Err(_) => Ok(()),
            };
            let _ = exit_write_sender.send(exit_result);
        }
    });

    let mut initial_write_complete = false;
    let mut initial_input_sent = false;
    let mut initial_input_due = None;
    let mut completion_output = None;
    let mut exit_write_complete = false;
    let mut closed_output = None;
    loop {
        if initial_input_due.is_none() {
            match readiness_receiver.try_recv() {
                Ok(()) => {
                    initial_input_due = Some(Instant::now() + initial_input_settle(output_mode));
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
            }
        }

        if !initial_input_sent && initial_input_due.is_some_and(|due| Instant::now() >= due) {
            if initial_command_sender.send(input.clone()).is_err() {
                return pty_failure_after_cleanup(&mut *child, cleanup_deadline, ProcessError::Io);
            }
            initial_input_sent = true;
        }

        if initial_input_sent && !initial_write_complete {
            match initial_write_receiver.try_recv() {
                Ok(Ok(())) => initial_write_complete = true,
                Ok(Err(error)) => {
                    return pty_failure_after_cleanup(&mut *child, cleanup_deadline, error);
                }
                Err(TryRecvError::Disconnected) => {
                    return pty_failure_after_cleanup(
                        &mut *child,
                        cleanup_deadline,
                        ProcessError::Io,
                    );
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        if completion_output.is_none() && closed_output.is_none() {
            match output_receiver.try_recv() {
                Ok(Ok(result)) => {
                    match stage_pty_output(result, &exit_input, &writer_command_sender) {
                        Ok(PtyOutputState::Completed(output)) => completion_output = Some(output),
                        Ok(PtyOutputState::Closed(output)) => closed_output = Some(output),
                        Err(error) => {
                            return pty_failure_after_cleanup(&mut *child, cleanup_deadline, error);
                        }
                    }
                }
                Ok(Err(error)) => {
                    return pty_failure_after_cleanup(&mut *child, cleanup_deadline, error);
                }
                Err(TryRecvError::Disconnected) => {
                    return pty_failure_after_cleanup(
                        &mut *child,
                        cleanup_deadline,
                        ProcessError::Io,
                    );
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        if completion_output.is_some() && !exit_write_complete {
            match exit_write_receiver.try_recv() {
                Ok(Ok(())) => exit_write_complete = true,
                Ok(Err(error)) => {
                    return pty_failure_after_cleanup(&mut *child, cleanup_deadline, error);
                }
                Err(TryRecvError::Disconnected) => {
                    return pty_failure_after_cleanup(
                        &mut *child,
                        cleanup_deadline,
                        ProcessError::Io,
                    );
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let exit_code = status.exit_code;
                if !initial_input_sent {
                    drop(initial_command_sender);
                }
                if let Some(output) = completion_output {
                    if initial_input_sent && !initial_write_complete {
                        receive_write_result_before(&initial_write_receiver, operation_deadline)?;
                    }
                    if !exit_write_complete {
                        receive_write_result_before(&exit_write_receiver, operation_deadline)?;
                    }
                    return pty_text(output);
                }
                if initial_input_sent && !initial_write_complete {
                    receive_write_result_before(&initial_write_receiver, operation_deadline)?;
                }
                if let Some(output) = closed_output {
                    drop(writer_command_sender);
                    return if status.success {
                        pty_text(output)
                    } else {
                        Err(ProcessError::NonZero(exit_code))
                    };
                }
                let remaining = operation_deadline.saturating_duration_since(Instant::now());
                return match output_receiver.recv_timeout(remaining) {
                    Ok(Ok(result)) => {
                        match stage_pty_output(result, &exit_input, &writer_command_sender)? {
                            PtyOutputState::Completed(output) => {
                                receive_write_result_before(
                                    &exit_write_receiver,
                                    operation_deadline,
                                )?;
                                pty_text(output)
                            }
                            PtyOutputState::Closed(output) if status.success => {
                                drop(writer_command_sender);
                                pty_text(output)
                            }
                            PtyOutputState::Closed(_) => {
                                drop(writer_command_sender);
                                Err(ProcessError::NonZero(exit_code))
                            }
                        }
                    }
                    Ok(Err(error)) => Err(error),
                    Err(_) if status.success => Err(ProcessError::Timeout),
                    Err(_) => Err(ProcessError::NonZero(exit_code)),
                };
            }
            Ok(None) => {}
            Err(_) => {
                return pty_failure_after_cleanup(&mut *child, cleanup_deadline, ProcessError::Io);
            }
        }

        if Instant::now() >= operation_deadline {
            terminate_pty_before(&mut *child, cleanup_deadline)?;
            if initial_input_sent && !initial_write_complete {
                receive_write_result_before(&initial_write_receiver, cleanup_deadline)?;
                initial_write_complete = true;
            }
            if completion_output.is_some() && !exit_write_complete {
                receive_write_result_before(&exit_write_receiver, cleanup_deadline)?;
                exit_write_complete = true;
            }
            if initial_input_sent && initial_write_complete && exit_write_complete {
                if let Some(output) = completion_output {
                    return pty_text(output);
                }
            }
            return Err(ProcessError::Timeout);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

enum PtyOutputState {
    Completed(Vec<u8>),
    Closed(Vec<u8>),
}

fn stage_pty_output(
    (output, completion_reached): (Vec<u8>, bool),
    exit_input: &Option<String>,
    writer_command_sender: &mpsc::Sender<Option<String>>,
) -> Result<PtyOutputState, ProcessError> {
    if completion_reached {
        writer_command_sender
            .send(exit_input.clone())
            .map_err(|_| ProcessError::Io)?;
        Ok(PtyOutputState::Completed(output))
    } else {
        Ok(PtyOutputState::Closed(output))
    }
}

fn receive_write_result_before(
    receiver: &mpsc::Receiver<Result<(), ProcessError>>,
    deadline: Instant,
) -> Result<(), ProcessError> {
    match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(ProcessError::Io),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(ProcessError::Timeout),
    }
}

fn read_pty_output_with_readiness(
    mut reader: Box<dyn Read + Send>,
    input: String,
    completion_markers: Vec<CompletionMarker>,
    output_mode: PtyOutputMode,
    readiness_sender: mpsc::Sender<()>,
) -> Result<(Vec<u8>, bool), ProcessError> {
    #[cfg(not(windows))]
    {
        let _ = output_mode;
        let _ = readiness_sender.send(());
    }
    let mut output = Vec::new();
    let mut buffer = [0_u8; 4096];
    #[cfg(windows)]
    let mut screen =
        matches!(output_mode, PtyOutputMode::RenderedScreen).then(|| vt100::Parser::new(30, 80, 0));
    #[cfg(windows)]
    let mut readiness_sent = false;

    loop {
        let count = reader.read(&mut buffer).map_err(map_spawn_error)?;
        if count == 0 {
            #[cfg(windows)]
            if let Some(screen) = screen.as_ref() {
                return Ok((
                    normalize_rendered_screen(&screen.screen().contents()).into_bytes(),
                    false,
                ));
            }
            return Ok((output, false));
        }
        if output.len().saturating_add(count) > MAX_OUTPUT_BYTES {
            return Err(ProcessError::OutputLimit);
        }
        output.extend_from_slice(&buffer[..count]);
        #[cfg(windows)]
        if let Some(screen) = screen.as_mut() {
            screen.process(&buffer[..count]);
            let rendered = normalize_rendered_screen(&screen.screen().contents());
            if !readiness_sent && !rendered.trim().is_empty() {
                let _ = readiness_sender.send(());
                readiness_sent = true;
            }
            if let Some(transcript) = rendered_screen_marker_transcript(
                &screen.screen().contents(),
                &input,
                &completion_markers,
            ) {
                return Ok((transcript, true));
            }
            continue;
        }
        if has_ordered_framed_completion_markers(&output, &input, &completion_markers) {
            return Ok((output, true));
        }
    }
}

fn normalize_rendered_screen(screen: &str) -> String {
    let mut normalized = screen
        .lines()
        .map(normalize_rendered_line)
        .collect::<Vec<_>>()
        .join("\n");
    if !normalized.is_empty() {
        normalized.push('\n');
    }
    normalized
}

fn normalize_rendered_line(line: &str) -> &str {
    let line = line.trim();
    let Some(first_digit) = line.find(|character: char| character.is_ascii_digit()) else {
        return line;
    };
    let (prefix, candidate) = line.split_at(first_digit);
    let Some(percent) = candidate.strip_suffix("% used") else {
        return line;
    };
    if !prefix.is_empty()
        && prefix.chars().all(|character| {
            character.is_whitespace() || ('\u{2580}'..='\u{259f}').contains(&character)
        })
        && !percent.is_empty()
        && percent.chars().all(|character| character.is_ascii_digit())
    {
        candidate
    } else {
        line
    }
}

fn rendered_screen_marker_transcript(
    screen: &str,
    input: &str,
    markers: &[CompletionMarker],
) -> Option<Vec<u8>> {
    let rendered = normalize_rendered_screen(screen);
    let lines = framed_output_lines(&rendered);
    let marker_range = completed_marker_range(&lines, input, markers)?;
    let mut transcript = lines[marker_range].join("\n");
    transcript.push('\n');
    Some(transcript.into_bytes())
}

fn has_ordered_framed_completion_markers(
    output: &[u8],
    input: &str,
    markers: &[CompletionMarker],
) -> bool {
    let output = strip_ansi_escapes::strip(output);
    let output = String::from_utf8_lossy(&output);
    let output_lines = framed_output_lines(&output);
    completed_marker_range(&output_lines, input, markers).is_some()
}

fn framed_output_lines(output: &str) -> Vec<&str> {
    output
        .split_inclusive('\n')
        .filter_map(|line| line.strip_suffix('\n'))
        .map(|line| line.trim_end_matches('\r'))
        .collect()
}

fn completed_marker_range(
    output_lines: &[&str],
    input: &str,
    markers: &[CompletionMarker],
) -> Option<std::ops::Range<usize>> {
    let input_lines = terminal_input_lines(input);
    let echoed_range = (!input_lines.is_empty())
        .then(|| {
            output_lines
                .windows(input_lines.len())
                .position(|lines| lines == input_lines)
        })
        .flatten()
        .map(|start| start..start + input_lines.len());

    if markers.is_empty() || markers.iter().any(|marker| !marker.is_valid()) {
        return None;
    }

    let mut marker_index = 0;
    let mut first_marker_line = None;
    for (line_index, line) in output_lines.iter().enumerate() {
        if echoed_range
            .as_ref()
            .is_some_and(|range| range.contains(&line_index))
        {
            continue;
        }
        if markers[marker_index].matches(line) {
            first_marker_line.get_or_insert(line_index);
            marker_index += 1;
            if marker_index == markers.len() {
                return Some(first_marker_line.expect("first marker was recorded")..line_index + 1);
            }
        }
    }
    None
}

fn terminal_input_lines(input: &str) -> Vec<&str> {
    let bytes = input.as_bytes();
    let mut lines = Vec::new();
    let mut line_start = 0;
    let mut index = 0;

    while index < bytes.len() {
        let delimiter_length = match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => 2,
            b'\r' | b'\n' => 1,
            _ => {
                index += 1;
                continue;
            }
        };

        lines.push(&input[line_start..index]);
        index += delimiter_length;
        line_start = index;
    }

    if line_start < input.len() {
        lines.push(&input[line_start..]);
    }
    lines
}

fn write_pty_input(writer: &mut dyn Write, input: &str) -> Result<(), ProcessError> {
    writer
        .write_all(input.as_bytes())
        .map_err(|_| ProcessError::Io)?;
    writer.flush().map_err(|_| ProcessError::Io)
}

fn write_exit_input_if_complete(
    writer: &mut dyn Write,
    completion_reached: bool,
    exit_input: Option<&str>,
) -> Result<bool, ProcessError> {
    let Some(exit_input) = exit_input.filter(|_| completion_reached) else {
        return Ok(false);
    };
    write_pty_input(writer, exit_input)?;
    Ok(true)
}

fn pty_text(output: Vec<u8>) -> Result<String, ProcessError> {
    bounded_text(strip_ansi_escapes::strip(output))
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
    use std::collections::VecDeque;

    use super::*;

    struct FakePtyChild {
        request_result: Result<(), ProcessError>,
        wait_results: VecDeque<Result<Option<PtyExitStatus>, ProcessError>>,
        requested: bool,
    }

    impl PtyChild for FakePtyChild {
        fn try_wait(&mut self) -> Result<Option<PtyExitStatus>, ProcessError> {
            self.wait_results.pop_front().unwrap_or(Ok(None))
        }

        fn request_termination(&mut self) -> Result<(), ProcessError> {
            self.requested = true;
            self.request_result.clone()
        }
    }

    #[test]
    fn pty_cleanup_returns_success_only_after_the_direct_child_is_reaped() {
        let mut child = FakePtyChild {
            request_result: Ok(()),
            wait_results: VecDeque::from([
                Ok(None),
                Ok(Some(PtyExitStatus {
                    success: false,
                    exit_code: 1,
                })),
            ]),
            requested: false,
        };

        assert_eq!(
            terminate_pty_before(&mut child, Instant::now() + Duration::from_millis(10)),
            Ok(())
        );
        assert!(child.requested);
    }

    #[test]
    fn pty_cleanup_propagates_termination_failure() {
        let mut child = FakePtyChild {
            request_result: Err(ProcessError::Io),
            wait_results: VecDeque::new(),
            requested: false,
        };

        assert_eq!(
            terminate_pty_before(&mut child, Instant::now() + Duration::from_millis(10)),
            Err(ProcessError::Io)
        );
        assert!(child.requested);
    }

    #[test]
    fn pty_cleanup_reports_timeout_when_the_direct_child_cannot_be_reaped_before_deadline() {
        let mut child = FakePtyChild {
            request_result: Ok(()),
            wait_results: VecDeque::new(),
            requested: false,
        };

        assert_eq!(
            terminate_pty_before(&mut child, Instant::now()),
            Err(ProcessError::Timeout)
        );
        assert!(child.requested);
    }

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
        assert_eq!(AllowedProgram::Winget.executable(), "winget");
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

        let before =
            windows_program_search_paths(Some(&startup_path_value), None, None, None, None, None);
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
            windows_program_search_paths(None, None, None, Some(&local_app_data), None, None);
        let launch = resolve_windows_program_from_paths(AllowedProgram::Claude, &paths).unwrap();

        assert_eq!(launch.executable, executable);
        assert!(launch.prefix_args.is_empty());
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
    fn exact_completion_marker_matches_only_a_whole_line() {
        let markers = vec![CompletionMarker::Exact("Current session".to_owned())];

        assert!(has_ordered_framed_completion_markers(
            b"Current session\r\n",
            "",
            &markers
        ));
        assert!(!has_ordered_framed_completion_markers(
            b"Current session remaining\r\n",
            "",
            &markers
        ));
    }

    #[test]
    fn exact_alternative_completion_marker_matches_only_approved_whole_lines() {
        let markers = vec![CompletionMarker::ExactAlternative {
            first: "All models".to_owned(),
            second: "Current week (all models)".to_owned(),
        }];

        assert!(has_ordered_framed_completion_markers(
            b"All models\r\n",
            "",
            &markers
        ));
        assert!(has_ordered_framed_completion_markers(
            b"Current week (all models)\r\n",
            "",
            &markers
        ));
        assert!(!has_ordered_framed_completion_markers(
            b"Current week (Sonnet)\r\n",
            "",
            &markers
        ));
        assert!(!has_ordered_framed_completion_markers(
            b"All models preview\r\n",
            "",
            &markers
        ));
    }

    #[test]
    fn prefix_completion_marker_matches_only_the_start_of_a_whole_line() {
        let markers = vec![CompletionMarker::Prefix("Resets ".to_owned())];

        assert!(has_ordered_framed_completion_markers(
            b"Resets Sep 1 at 10:00\r\n",
            "",
            &markers
        ));
        assert!(!has_ordered_framed_completion_markers(
            b"Usage Resets Sep 1 at 10:00\r\n",
            "",
            &markers
        ));
    }

    #[test]
    fn completion_markers_must_appear_in_supplied_order() {
        let markers = vec![
            CompletionMarker::Exact("Current session".to_owned()),
            CompletionMarker::Exact("Current week (all models)".to_owned()),
        ];

        assert!(!has_ordered_framed_completion_markers(
            b"Current week (all models)\r\nCurrent session\r\n",
            "",
            &markers
        ));
        assert!(has_ordered_framed_completion_markers(
            b"Current session\r\nCurrent week (all models)\r\n",
            "",
            &markers
        ));
    }

    #[test]
    fn reset_before_weekly_heading_cannot_satisfy_later_prefix_marker() {
        let markers = vec![
            CompletionMarker::Exact("Current week (all models)".to_owned()),
            CompletionMarker::Prefix("Resets ".to_owned()),
        ];

        assert!(!has_ordered_framed_completion_markers(
            b"Resets Aug 30 at 12:00\r\nCurrent week (all models)\r\n",
            "",
            &markers
        ));
    }

    #[test]
    fn weekly_heading_and_unterminated_reset_line_do_not_complete() {
        let markers = vec![
            CompletionMarker::Exact("Current week (all models)".to_owned()),
            CompletionMarker::Prefix("Resets ".to_owned()),
        ];

        assert!(!has_ordered_framed_completion_markers(
            b"Current week (all models)\r\n",
            "",
            &markers
        ));
        assert!(!has_ordered_framed_completion_markers(
            b"Current week (all models)\r\nResets Sep 1 at 10:00",
            "",
            &markers
        ));
    }

    #[test]
    fn terminated_weekly_reset_line_completes_ordered_markers() {
        let markers = vec![
            CompletionMarker::Exact("Current week (all models)".to_owned()),
            CompletionMarker::Prefix("Resets ".to_owned()),
        ];

        assert!(has_ordered_framed_completion_markers(
            b"Current week (all models)\r\n42% used\r\nResets Sep 1 at 10:00\r\n",
            "",
            &markers
        ));
    }

    #[test]
    fn ordered_typed_markers_ignore_the_confirmed_input_echo() {
        let markers = vec![
            CompletionMarker::Exact("SESSION".to_owned()),
            CompletionMarker::Exact("WEEK".to_owned()),
            CompletionMarker::Prefix("Resets ".to_owned()),
        ];
        let input = "SESSION\rWEEK\rResets echoed\r";

        assert!(!has_ordered_framed_completion_markers(
            b"SESSION\r\nWEEK\r\nResets echoed\r\n",
            input,
            &markers
        ));
        assert!(has_ordered_framed_completion_markers(
            b"SESSION\r\nWEEK\r\nResets echoed\r\nSESSION\r\nWEEK\r\nResets Sep 1\r\n",
            input,
            &markers
        ));
    }

    #[test]
    fn completion_markers_ignore_lone_cr_echoed_input() {
        let markers = vec![
            CompletionMarker::Exact("SESSION".to_owned()),
            CompletionMarker::Exact("WEEK".to_owned()),
        ];
        let input = "SESSION\rWEEK\r";

        assert!(!has_ordered_framed_completion_markers(
            b"SESSION\r\nWEEK\r\n",
            input,
            &markers
        ));
        assert!(has_ordered_framed_completion_markers(
            b"SESSION\r\nWEEK\r\nprovider response\r\nSESSION\r\nWEEK\r\n",
            input,
            &markers
        ));
    }

    #[test]
    fn completion_markers_ignore_crlf_echoed_input() {
        let markers = vec![
            CompletionMarker::Exact("SESSION".to_owned()),
            CompletionMarker::Exact("WEEK".to_owned()),
        ];
        let input = "SESSION\r\nWEEK\r\n";

        assert!(!has_ordered_framed_completion_markers(
            b"SESSION\r\nWEEK\r\n",
            input,
            &markers
        ));
    }

    #[test]
    fn completion_markers_preserve_intentional_blank_echoed_lines() {
        let markers = vec![
            CompletionMarker::Exact("SESSION".to_owned()),
            CompletionMarker::Exact("WEEK".to_owned()),
        ];
        let input = "SESSION\r\rWEEK\r";

        assert!(!has_ordered_framed_completion_markers(
            b"SESSION\r\n\r\nWEEK\r\n",
            input,
            &markers
        ));
    }

    #[test]
    fn completion_observed_after_natural_exit_stages_exit_input() {
        let (sender, receiver) = mpsc::channel();
        let exit_input = Some("/exit\r".to_owned());

        let state = stage_pty_output(
            (b"SESSION\r\nWEEK\r\n".to_vec(), true),
            &exit_input,
            &sender,
        )
        .unwrap();

        assert!(matches!(
            state,
            PtyOutputState::Completed(output) if output == b"SESSION\r\nWEEK\r\n"
        ));
        assert_eq!(receiver.try_recv().unwrap(), exit_input);
    }

    #[test]
    fn staged_write_acknowledgement_preserves_io_errors() {
        let (sender, receiver) = mpsc::channel();
        sender.send(Err(ProcessError::Io)).unwrap();

        assert_eq!(
            receive_write_result_before(&receiver, Instant::now() + Duration::from_secs(1)),
            Err(ProcessError::Io)
        );
    }

    #[test]
    fn completion_requires_every_marker() {
        let markers = vec![
            CompletionMarker::Exact("SESSION".to_owned()),
            CompletionMarker::Exact("WEEK".to_owned()),
        ];

        assert!(!has_ordered_framed_completion_markers(
            b"SESSION\r\n",
            "status\n",
            &markers
        ));
        assert!(!has_ordered_framed_completion_markers(
            b"WEEK\r\n",
            "status\n",
            &markers
        ));
        assert!(has_ordered_framed_completion_markers(
            b"SESSION\r\nWEEK\r\n",
            "status\n",
            &markers
        ));
    }

    #[test]
    fn absent_or_empty_completion_markers_do_not_complete() {
        assert!(!has_ordered_framed_completion_markers(
            b"anything\r\n",
            "",
            &[]
        ));
        assert!(!has_ordered_framed_completion_markers(
            b"anything\r\n",
            "",
            &[CompletionMarker::Exact(String::new())]
        ));
        assert!(!has_ordered_framed_completion_markers(
            b"anything\r\n",
            "",
            &[CompletionMarker::Prefix(String::new())]
        ));
    }

    #[test]
    fn completion_markers_require_terminated_standalone_lines() {
        let markers = vec![
            CompletionMarker::Exact("SESSION".to_owned()),
            CompletionMarker::Exact("WEEK".to_owned()),
        ];

        assert!(!has_ordered_framed_completion_markers(
            b"SESSION\r\nWEEK",
            "",
            &markers
        ));
        assert!(!has_ordered_framed_completion_markers(
            b"SESSION\r\nprovider says WEEK soon\r\n",
            "",
            &markers
        ));
        assert!(has_ordered_framed_completion_markers(
            b"SESSION\r\nWEEK\r\n",
            "",
            &markers
        ));
    }

    #[test]
    fn exit_input_is_written_only_after_completion_when_present() {
        let mut writer = Vec::new();

        assert!(!write_exit_input_if_complete(&mut writer, false, Some("/exit\r")).unwrap());
        assert!(!write_exit_input_if_complete(&mut writer, true, None).unwrap());
        assert!(write_exit_input_if_complete(&mut writer, true, Some("/exit\r")).unwrap());
        assert_eq!(writer, b"/exit\r");
    }

    #[test]
    fn pty_input_write_and_flush_errors_map_to_io() {
        struct FailingWriter {
            fail_flush: bool,
        }

        impl Write for FailingWriter {
            fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
                if self.fail_flush {
                    Ok(buffer.len())
                } else {
                    Err(std::io::Error::other("write failed"))
                }
            }

            fn flush(&mut self) -> std::io::Result<()> {
                if self.fail_flush {
                    Err(std::io::Error::other("flush failed"))
                } else {
                    Ok(())
                }
            }
        }

        assert_eq!(
            write_pty_input(&mut FailingWriter { fail_flush: false }, "status\r"),
            Err(ProcessError::Io)
        );
        assert_eq!(
            write_pty_input(&mut FailingWriter { fail_flush: true }, "status\r"),
            Err(ProcessError::Io)
        );
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

    #[cfg(windows)]
    #[test]
    fn windows_interactive_commands_select_the_flags_zero_conpty_backend() {
        assert_eq!(interactive_pty_backend(), InteractivePtyBackend::Conpty);
    }

    #[cfg(windows)]
    #[test]
    fn rendered_windows_screen_normalizes_padding_before_matching_markers() {
        let rendered = "  Current session  \n  23% used  \n  Resets in 2 hr  \n\n  Current week (all models)  \n  41% used  \n  Resets Sep 3 at 2:00 PM  \n";
        let markers = vec![
            CompletionMarker::Exact("Current session".to_owned()),
            CompletionMarker::Exact("Current week (all models)".to_owned()),
            CompletionMarker::Prefix("Resets ".to_owned()),
        ];

        assert!(has_ordered_framed_completion_markers(
            normalize_rendered_screen(rendered).as_bytes(),
            "/usage\r",
            &markers,
        ));
    }

    #[cfg(windows)]
    #[test]
    fn rendered_windows_pty_reader_signals_readiness_from_a_nonblank_screen() {
        let (sender, receiver) = mpsc::channel();
        let (_, completed) = read_pty_output_with_readiness(
            Box::new(std::io::Cursor::new(b"\x1b[2J\x1b[H  ready  ".to_vec())),
            String::new(),
            Vec::new(),
            PtyOutputMode::RenderedScreen,
            sender,
        )
        .unwrap();

        assert!(!completed);
        assert!(receiver.try_recv().is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn rendered_windows_screen_returns_only_the_completed_marker_transcript() {
        let rendered = "  terminal chrome  \n  Current session  \n  23% used  \n  Resets in 2 hr  \n\n  Current week (all models)  \n  41% used  \n  Resets Sep 3 at 2:00 PM  \n  terminal footer  \n";
        let markers = vec![
            CompletionMarker::Exact("Current session".to_owned()),
            CompletionMarker::Exact("Current week (all models)".to_owned()),
            CompletionMarker::Prefix("Resets ".to_owned()),
        ];

        assert_eq!(
            rendered_screen_marker_transcript(rendered, "/usage\r", &markers),
            Some(
                "Current session\n23% used\nResets in 2 hr\n\nCurrent week (all models)\n41% used\nResets Sep 3 at 2:00 PM\n"
                    .as_bytes()
                    .to_vec()
            )
        );
    }

    #[cfg(windows)]
    #[test]
    fn rendered_windows_screen_removes_only_terminal_progress_prefixes() {
        let rendered =
            "  Current session  \n  \u{2588}\u{2588} 23% used  \n  progress 41% used  \n";

        assert_eq!(
            normalize_rendered_screen(rendered),
            "Current session\n23% used\nprogress 41% used\n"
        );
    }
}
