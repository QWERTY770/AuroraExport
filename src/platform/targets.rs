pub(crate) fn normalized_target(target: &str) -> Result<String, String> {
    let target = target.trim();
    if target.is_empty() || target.contains('\0') {
        return Err("invalid or empty target".into());
    }
    if target.starts_with("https://") || target.starts_with("http://") {
        let parsed = url::Url::parse(target).map_err(|error| error.to_string())?;
        if parsed.host_str().is_none() {
            return Err("URL needs a host".into());
        }
        return Ok(parsed.to_string());
    }
    let path = std::path::Path::new(target);
    if !path.exists() {
        return Err(format!("path does not exist: {target}"));
    }
    let path = std::path::absolute(path).map_err(|error| error.to_string())?;
    let path = path.to_string_lossy().into_owned();
    #[cfg(windows)]
    let path = shell_path(&path)?;
    Ok(path)
}

#[cfg(windows)]
fn shell_path(path: &str) -> Result<String, String> {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        Ok(format!(r"\\{unc}"))
    } else if let Some(disk) = path.strip_prefix(r"\\?\") {
        let bytes = disk.as_bytes();
        if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && &bytes[1..3] == b":\\" {
            Ok(disk.to_string())
        } else {
            Err("this device path cannot be opened by the desktop shell".into())
        }
    } else {
        Ok(path.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_target_is_rejected_before_launching() {
        assert!(normalized_target("   ").is_err());
    }

    #[test]
    fn missing_target_is_rejected_before_launching() {
        let path =
            std::env::temp_dir().join(format!("aurora-missing-normalized-{}", std::process::id()));
        assert!(normalized_target(&path.to_string_lossy()).is_err());
    }

    #[test]
    fn urls_are_validated_without_opening_a_browser() {
        assert_eq!(
            normalized_target(" https://uigf.org/ ").unwrap(),
            "https://uigf.org/"
        );
        assert!(normalized_target("https://[invalid").is_err());
        assert!(normalized_target("https://uigf.org/\0").is_err());
        assert!(normalized_target("javascript:alert(1)").is_err());
    }

    #[test]
    fn paths_are_absolute_before_handing_them_to_the_shell() {
        let target = normalized_target(".").unwrap();
        assert!(std::path::Path::new(&target).is_absolute());
    }

    #[cfg(windows)]
    #[test]
    fn desktop_paths_do_not_use_filesystem_only_prefixes() {
        assert_eq!(
            shell_path(r"\\?\D:\软件\userData").unwrap(),
            r"D:\软件\userData"
        );
        assert_eq!(
            shell_path(r"\\?\UNC\server\share\userData").unwrap(),
            r"\\server\share\userData"
        );
        assert_eq!(shell_path(r"D:\userData").unwrap(), r"D:\userData");
        assert!(shell_path(r"\\?\GLOBALROOT\Device\HarddiskVolume1").is_err());
        let path = std::fs::canonicalize(".").unwrap();
        let target = normalized_target(&path.to_string_lossy()).unwrap();
        assert!(!target.starts_with(r"\\?\"));
        assert!(std::path::Path::new(&target).is_dir());
    }
}
