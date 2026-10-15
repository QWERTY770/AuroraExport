use std::path::{Path, PathBuf};

pub fn writable_directory(directory: &Path) -> bool {
    probe_directory(directory).is_ok()
}

pub fn probe_directory(directory: &Path) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_PROBE: AtomicU64 = AtomicU64::new(0);
    if !std::fs::metadata(directory)?.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            "target is not an accessible directory",
        ));
    }
    let sequence = NEXT_PROBE.fetch_add(1, Ordering::Relaxed);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = directory.join(format!(
        ".aurora-write-{}-{timestamp}-{sequence}",
        std::process::id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let write_result = file
        .write_all(b"AuroraExport write probe\n")
        .and_then(|()| file.flush());
    drop(file);
    let cleanup_result = std::fs::remove_file(path);
    write_result?;
    cleanup_result
}

pub fn selected_path(mut path: PathBuf, extension: &str) -> PathBuf {
    if path
        .extension()
        .is_none_or(|extension| extension.is_empty())
    {
        path.set_extension(extension);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_is_only_added_when_missing() {
        assert_eq!(
            selected_path("records".into(), "json"),
            PathBuf::from("records.json")
        );
        assert_eq!(
            selected_path("records.JSON".into(), "json"),
            PathBuf::from("records.JSON")
        );
        assert_eq!(
            selected_path("记录".into(), "xlsx"),
            PathBuf::from("记录.xlsx")
        );
        assert_eq!(
            selected_path("records.".into(), "json"),
            PathBuf::from("records.json")
        );
        assert_eq!(
            selected_path("records.txt".into(), "json"),
            PathBuf::from("records.txt")
        );
    }

    #[test]
    fn writable_probe_leaves_no_files() {
        let root = std::env::var_os("AURORA_TEST_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let directory = root.join(format!("aurora-dialog-test-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        assert!(writable_directory(&directory));
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
        assert!(!writable_directory(&directory.join("missing")));
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn probing_never_overwrites_an_existing_file() {
        let root = std::env::var_os("AURORA_TEST_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let directory = root.join(format!("aurora-probe-existing-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let existing = directory.join("records.json");
        std::fs::write(&existing, b"keep this data").unwrap();
        probe_directory(&directory).unwrap();
        assert_eq!(std::fs::read(&existing).unwrap(), b"keep this data");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        assert_eq!(
            probe_directory(&existing).unwrap_err().kind(),
            std::io::ErrorKind::NotADirectory
        );
        std::fs::remove_file(existing).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
