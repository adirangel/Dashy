//! A local, bounded log of provider refreshes for troubleshooting.
//!
//! Each refresh writes one line: when it ran, which provider, whether it
//! succeeded (or the sanitized error category), and how long it took. Nothing
//! else is recorded — no CLI output, arguments, account identity, or secrets —
//! so the file can be shared as-is when a tile shows "Unavailable".

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use chrono::{DateTime, SecondsFormat, Utc};

use crate::dashboard::models::ProviderId;

pub const LOG_FILE_NAME: &str = "dashy.log";
pub const ROTATED_LOG_FILE_NAME: &str = "dashy.log.1";
/// The live log rotates to the `.1` file once it grows past this size, so the
/// pair never exceeds roughly half a megabyte.
pub const MAX_LOG_BYTES: u64 = 256 * 1024;

pub struct RefreshRecord {
    pub at: DateTime<Utc>,
    pub provider: ProviderId,
    /// `None` when the refresh produced data; otherwise the provider error's
    /// display text, which is a fixed category and never CLI output.
    pub error: Option<String>,
    pub duration: Duration,
}

pub trait DiagnosticsSink: Send + Sync {
    fn record(&self, record: &RefreshRecord);
}

/// Discards every record; the default until a log directory is attached.
pub struct NoopDiagnostics;

impl DiagnosticsSink for NoopDiagnostics {
    fn record(&self, _record: &RefreshRecord) {}
}

pub fn provider_name(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::Claude => "claude",
        ProviderId::Codex => "codex",
        ProviderId::GitHub => "github",
        ProviderId::Grok => "grok",
        ProviderId::Cursor => "cursor",
    }
}

pub fn format_line(record: &RefreshRecord) -> String {
    let outcome = match &record.error {
        None => "ok".to_owned(),
        Some(error) => format!("error: {error}"),
    };
    format!(
        "{} {} {} {}ms\n",
        record.at.to_rfc3339_opts(SecondsFormat::Secs, true),
        provider_name(record.provider),
        outcome,
        record.duration.as_millis()
    )
}

/// Appends records to `dashy.log` inside an attached directory, rotating the
/// file once it exceeds [`MAX_LOG_BYTES`]. Failures are swallowed on purpose:
/// diagnostics must never affect the dashboard itself.
pub struct FileDiagnostics {
    state: Mutex<FileDiagnosticsState>,
}

struct FileDiagnosticsState {
    directory: Option<PathBuf>,
    max_bytes: u64,
}

impl Default for FileDiagnostics {
    fn default() -> Self {
        Self::new()
    }
}

impl FileDiagnostics {
    pub fn new() -> Self {
        Self::with_limit(MAX_LOG_BYTES)
    }

    pub fn with_limit(max_bytes: u64) -> Self {
        Self {
            state: Mutex::new(FileDiagnosticsState {
                directory: None,
                max_bytes,
            }),
        }
    }

    /// Starts writing into `directory` (created on demand). Records that
    /// arrive before this call are dropped.
    pub fn attach_directory(&self, directory: PathBuf) {
        if let Ok(mut state) = self.state.lock() {
            state.directory = Some(directory);
        }
    }

    pub fn log_path(&self) -> Option<PathBuf> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.directory.as_ref().map(|dir| dir.join(LOG_FILE_NAME)))
    }

    fn append(directory: &Path, max_bytes: u64, line: &str) -> std::io::Result<()> {
        fs::create_dir_all(directory)?;
        let path = directory.join(LOG_FILE_NAME);
        if fs::metadata(&path)
            .map(|meta| meta.len() > max_bytes)
            .unwrap_or(false)
        {
            fs::rename(&path, directory.join(ROTATED_LOG_FILE_NAME))?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(line.as_bytes())
    }
}

impl DiagnosticsSink for FileDiagnostics {
    fn record(&self, record: &RefreshRecord) {
        let Ok(state) = self.state.lock() else {
            return;
        };
        let Some(directory) = state.directory.as_deref() else {
            return;
        };
        let _ = Self::append(directory, state.max_bytes, &format_line(record));
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn record(provider: ProviderId, error: Option<&str>, millis: u64) -> RefreshRecord {
        RefreshRecord {
            at: Utc.with_ymd_and_hms(2026, 9, 1, 19, 55, 2).unwrap(),
            provider,
            error: error.map(str::to_owned),
            duration: Duration::from_millis(millis),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let unique = format!(
            "dashy-diagnostics-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }

    #[test]
    fn lines_carry_only_time_provider_outcome_and_duration() {
        assert_eq!(
            format_line(&record(ProviderId::Claude, None, 4123)),
            "2026-09-01T19:55:02Z claude ok 4123ms\n"
        );
        assert_eq!(
            format_line(&record(
                ProviderId::Grok,
                Some("provider authentication is unavailable"),
                812
            )),
            "2026-09-01T19:55:02Z grok error: provider authentication is unavailable 812ms\n"
        );
    }

    #[test]
    fn records_before_a_directory_is_attached_are_dropped() {
        let sink = FileDiagnostics::new();
        sink.record(&record(ProviderId::Codex, None, 1));
        assert!(sink.log_path().is_none());
    }

    #[test]
    fn appends_lines_and_rotates_once_the_file_outgrows_its_limit() {
        let dir = temp_dir("rotate");
        let sink = FileDiagnostics::with_limit(60);
        sink.attach_directory(dir.clone());

        sink.record(&record(ProviderId::Claude, None, 10)); // 36 bytes
        sink.record(&record(ProviderId::Codex, None, 20)); // 71 bytes, over the limit
        sink.record(&record(ProviderId::GitHub, None, 30)); // rotates first

        let live = fs::read_to_string(dir.join(LOG_FILE_NAME)).unwrap();
        let rotated = fs::read_to_string(dir.join(ROTATED_LOG_FILE_NAME)).unwrap();
        assert_eq!(live, "2026-09-01T19:55:02Z github ok 30ms\n");
        assert_eq!(
            rotated,
            "2026-09-01T19:55:02Z claude ok 10ms\n2026-09-01T19:55:02Z codex ok 20ms\n"
        );
        assert_eq!(sink.log_path(), Some(dir.join(LOG_FILE_NAME)));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unwritable_directory_never_panics() {
        let sink = FileDiagnostics::new();
        // A file where the directory should be makes create_dir_all fail.
        let blocker = temp_dir("blocked");
        fs::write(&blocker, "not a directory").unwrap();
        sink.attach_directory(blocker.clone());
        sink.record(&record(ProviderId::Cursor, None, 5));
        fs::remove_file(blocker).unwrap();
    }
}
