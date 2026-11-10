use std::{
    fmt::Display,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct DiagnosticLog {
    file: Mutex<File>,
}

impl DiagnosticLog {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    pub fn record(&self, context: &str, details: impl Display) -> io::Result<()> {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("el registro local está bloqueado"))?;

        writeln!(file, "{timestamp_ms}\t{context}\t{details}")
    }
}

#[cfg(test)]
mod tests {
    use super::DiagnosticLog;
    use std::{fs, time::SystemTime};

    #[test]
    fn appends_diagnostics_to_local_file() {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-log-{unique}"));
        let path = directory.join("logs").join("ariadna.log");
        let log = DiagnosticLog::open(&path).unwrap();

        log.record("test.operation", "fallo controlado").unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("test.operation\tfallo controlado"));
        drop(log);
        fs::remove_dir_all(directory).unwrap();
    }
}
