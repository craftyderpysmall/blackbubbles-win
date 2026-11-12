/// Save downloaded bytes into the user's Downloads folder without overwriting.
pub fn save_download(name: &str, data: &[u8]) -> std::io::Result<std::path::PathBuf> {
    let dir = directories::UserDirs::new()
        .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&dir)?;
    let clean: String = name.chars().filter(|c| !c.is_control() && *c != '/' && *c != '\\').collect();
    let clean = if clean.trim().is_empty() { "download".to_string() } else { clean };
    let mut path = dir.join(&clean);
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{n} {clean}"));
        n += 1;
    }
    std::fs::write(&path, data)?;
    Ok(path)
}

