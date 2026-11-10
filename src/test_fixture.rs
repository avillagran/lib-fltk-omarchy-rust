//! Fixture directories for unit tests.
//!
//! Every call creates a brand-new directory with `create_dir` (fails if
//! anything, including a symlink, already has the name), so a test never
//! writes into a directory it did not create. Tests never delete a tree:
//! they may remove single files they wrote themselves and the directory
//! itself with `remove_dir`, which only succeeds when it is empty. A
//! guard test (`tests/no_tree_deletion.rs`) keeps recursive deletion out
//! of the sources.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// A new, empty, private directory under the system temp dir (kept short:
/// some fixtures bind Unix sockets inside it).
pub(crate) fn dir(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let base = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    loop {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d = base.join(format!("fo-{tag}-{}-{nanos:x}-{n}", std::process::id()));
        match std::fs::create_dir(&d) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
                }
                return d;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => panic!("test fixture {}: {e}", d.display()),
        }
    }
}

/// Removes the named files (only regular files or sockets, never through a
/// symlink) and then the directory if it is empty. Anything else stays.
pub(crate) fn tidy(dir: &Path, files: &[&str]) {
    for f in files {
        let p = dir.join(f);
        if std::fs::symlink_metadata(&p).is_ok_and(|m| !m.is_dir()) {
            let _ = std::fs::remove_file(&p);
        }
    }
    let _ = std::fs::remove_dir(dir);
}
