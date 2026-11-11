use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_BACKUP_FILES: usize = 3;
const MAX_FIELD_CHARS: usize = 384;
const MIN_POLICY_BYTES: u64 = 1024;

#[derive(Clone, Copy)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

pub struct LogEvent<'a> {
    pub level: LogLevel,
    pub operation_id: &'a str,
    pub command: &'a str,
    pub result: &'a str,
    pub duration_ms: Option<u128>,
    pub error_code: Option<&'a str>,
    pub detail: Option<&'a str>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    timestamp_ms: u128,
    level: &'static str,
    operation_id: String,
    command: String,
    result: String,
    duration_ms: Option<u128>,
    error_code: Option<String>,
    detail: Option<String>,
}

struct LogState {
    file: Option<File>,
}

pub struct DiagnosticLog {
    path: PathBuf,
    max_file_bytes: u64,
    backup_files: usize,
    state: Mutex<LogState>,
}

impl DiagnosticLog {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_with_policy(path, MAX_FILE_BYTES, MAX_BACKUP_FILES)
    }

    fn open_with_policy(
        path: impl AsRef<Path>,
        max_file_bytes: u64,
        backup_files: usize,
    ) -> io::Result<Self> {
        if max_file_bytes < MIN_POLICY_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "el límite del registro debe ser de al menos 1024 bytes",
            ));
        }

        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        if fs::metadata(&path)
            .map(|metadata| metadata.len() > max_file_bytes)
            .unwrap_or(false)
        {
            rotate_files(&path, backup_files)?;
        }

        let file = open_append(&path)?;
        Ok(Self {
            path,
            max_file_bytes,
            backup_files,
            state: Mutex::new(LogState { file: Some(file) }),
        })
    }

    pub fn next_operation_id() -> String {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        format!("op-{timestamp_ns:x}-{sequence:x}")
    }

    pub fn record_event(&self, event: LogEvent<'_>) -> io::Result<()> {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let entry = LogEntry {
            timestamp_ms,
            level: event.level.as_str(),
            operation_id: sanitize_field(event.operation_id),
            command: sanitize_field(event.command),
            result: sanitize_field(event.result),
            duration_ms: event.duration_ms,
            error_code: event.error_code.map(sanitize_field),
            detail: event.detail.map(sanitize_field),
        };
        let mut line = serde_json::to_vec(&entry).map_err(|error| {
            io::Error::other(format!("no se pudo serializar el registro: {error}"))
        })?;
        line.push(b'\n');

        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("el registro local está bloqueado"))?;
        let current_len = state
            .file
            .as_ref()
            .ok_or_else(|| io::Error::other("el archivo de registro no está disponible"))?
            .metadata()?
            .len();
        if current_len > 0 && current_len.saturating_add(line.len() as u64) > self.max_file_bytes {
            state.file.take();
            let rotation_result = rotate_files(&self.path, self.backup_files);
            state.file = Some(open_append(&self.path)?);
            rotation_result?;
        }

        let file = state
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("el archivo de registro no está disponible"))?;
        file.write_all(&line)?;
        file.flush()
    }
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn backup_path(path: &Path, index: usize) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(format!(".{index}"));
    PathBuf::from(value)
}

fn rotate_files(path: &Path, backup_files: usize) -> io::Result<()> {
    if backup_files == 0 {
        return remove_if_exists(path);
    }

    let last_backup = backup_path(path, backup_files);
    remove_if_exists(&last_backup)?;

    for index in (1..=backup_files).rev() {
        let source = if index == 1 {
            path.to_path_buf()
        } else {
            backup_path(path, index - 1)
        };
        if !source.exists() {
            continue;
        }
        let destination = backup_path(path, index);
        remove_if_exists(&destination)?;
        fs::rename(source, destination)?;
    }
    Ok(())
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn sanitize_field(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(MAX_FIELD_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticLog, LogEvent, LogLevel};
    use serde_json::Value;
    use std::{fs, path::PathBuf, time::SystemTime};

    fn temp_path(label: &str) -> (PathBuf, PathBuf) {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-{label}-{unique}"));
        let path = directory.join("logs").join("ariadna.log");
        (directory, path)
    }

    fn write(log: &DiagnosticLog, operation_id: &str, detail: Option<&str>) {
        log.record_event(LogEvent {
            level: LogLevel::Error,
            operation_id,
            command: "ipc.inventory.search",
            result: "error",
            duration_ms: Some(17),
            error_code: Some("storage"),
            detail,
        })
        .unwrap();
    }

    #[test]
    fn records_structured_correlated_events_without_untrusted_detail() {
        let (directory, path) = temp_path("log-structured");
        let log = DiagnosticLog::open(&path).unwrap();
        write(&log, "op-123-1", Some("detalle\tinyectado\nsegunda línea"));

        let contents = fs::read_to_string(&path).unwrap();
        let entry: Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(entry["operationId"], "op-123-1");
        assert_eq!(entry["command"], "ipc.inventory.search");
        assert_eq!(entry["result"], "error");
        assert_eq!(entry["durationMs"], 17);
        assert_eq!(entry["errorCode"], "storage");
        assert_eq!(entry["detail"], "detalle inyectado segunda línea");
        assert_eq!(contents.lines().count(), 1);

        drop(log);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn truncates_untrusted_log_fields() {
        let (directory, path) = temp_path("log-truncate");
        let log = DiagnosticLog::open(&path).unwrap();
        let detail = "x".repeat(1000);
        write(&log, "op-1", Some(&detail));

        let contents = fs::read_to_string(&path).unwrap();
        let entry: Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(entry["detail"].as_str().unwrap().chars().count(), 384);

        drop(log);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rotates_files_and_keeps_only_the_configured_backups() {
        let (directory, path) = temp_path("log-rotation");
        let log = DiagnosticLog::open_with_policy(&path, 1024, 2).unwrap();

        for index in 0..16 {
            write(&log, &format!("op-{index}"), Some(&"d".repeat(180)));
        }

        drop(log);
        assert!(path.exists());
        assert!(super::backup_path(&path, 1).exists());
        assert!(super::backup_path(&path, 2).exists());
        assert!(!super::backup_path(&path, 3).exists());
        assert!(fs::metadata(&path).unwrap().len() <= 1024);
        assert!(fs::read_to_string(&path).unwrap().contains("op-15"));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn generates_distinct_operation_ids() {
        let first = DiagnosticLog::next_operation_id();
        let second = DiagnosticLog::next_operation_id();
        assert_ne!(first, second);
    }
}
