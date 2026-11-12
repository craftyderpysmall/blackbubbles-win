//! Find My for BlackBubbles: your Apple devices (and optionally your family's), via the
//! iCloud web Find My API ([`icloud`]), signed in with Apple's SRP ([`srp`]).
//!
//! Everything here talks to undocumented Apple endpoints and may break without notice.

pub mod icloud;
pub mod srp;

use std::path::Path;

/// Write a file readable only by the current user.
pub(crate) fn write_private(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(data)?;
    Ok(())
}
