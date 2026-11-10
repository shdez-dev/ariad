use std::{fs, io, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppConfig {
    data_directory: PathBuf,
    database_path: PathBuf,
    log_path: PathBuf,
}

impl AppConfig {
    pub fn from_data_directory(data_directory: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&data_directory)?;
        let log_directory = data_directory.join("logs");
        fs::create_dir_all(&log_directory)?;

        Ok(Self {
            database_path: data_directory.join("ariadna.sqlite3"),
            log_path: log_directory.join("ariadna.log"),
            data_directory,
        })
    }

    pub fn data_directory(&self) -> &PathBuf {
        &self.data_directory
    }

    pub fn database_path(&self) -> &PathBuf {
        &self.database_path
    }

    pub fn log_path(&self) -> &PathBuf {
        &self.log_path
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;
    use std::{fs, time::SystemTime};

    #[test]
    fn creates_local_database_and_log_paths() {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-config-{unique}"));

        let config = AppConfig::from_data_directory(directory.clone()).unwrap();

        assert_eq!(config.database_path(), &directory.join("ariadna.sqlite3"));
        assert_eq!(
            config.log_path(),
            &directory.join("logs").join("ariadna.log")
        );
        assert!(directory.is_dir());
        assert!(directory.join("logs").is_dir());

        fs::remove_dir_all(directory).unwrap();
    }
}
