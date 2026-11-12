/// A file name Windows accepts: no control characters, no `\ / : * ? " < > |`, no trailing
/// dots or spaces (Explorer can't handle those). Falls back to "download".
pub fn clean_file_name(name: &str) -> String {
    let clean: String = name.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '-' } else { c }).collect();
    let clean = clean.trim().trim_end_matches(['.', ' ']).to_string();
    if clean.is_empty() { "download".to_string() } else { clean }
}

/// Save downloaded bytes into the user's Downloads folder without overwriting.
pub fn save_download(name: &str, data: &[u8]) -> std::io::Result<std::path::PathBuf> {
    let dir = directories::UserDirs::new()
        .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&dir)?;
    let clean = clean_file_name(name);
    let mut path = dir.join(&clean);
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{n} {clean}"));
        n += 1;
    }
    std::fs::write(&path, data)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::clean_file_name;

    #[test]
    fn file_names_windows_accepts() {
        assert_eq!(clean_file_name("Meeting: 2/3 notes?.m4a"), "Meeting- 2-3 notes-.m4a");
        assert_eq!(clean_file_name("photo.jpg"), "photo.jpg");
        assert_eq!(clean_file_name("trailing. "), "trailing");
        assert_eq!(clean_file_name("  "), "download");
        assert_eq!(clean_file_name("<>|"), "---");
    }
}
