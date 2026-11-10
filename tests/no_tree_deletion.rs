//! Guard: no source, test, example, template or runner script deletes a
//! directory tree. Fixtures use new directories created with `create_dir`
//! and, at most, remove single files they wrote plus empty directories
//! (`src/test_fixture.rs`, `retire` in `tests/ui.rs`). A test that needs a
//! "removed" path moves it aside instead.
use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &["remove_dir_all", "rm -rf", "rm -fr", "rm -r ", "rmtree", "shutil.rmtree"];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    // symlink_metadata: never follow a link out of the checkout.
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(m) = std::fs::symlink_metadata(&p) else { continue };
        if m.is_dir() {
            walk(&p, out);
        } else if m.is_file() {
            out.push(p);
        }
    }
}

#[test]
fn nothing_deletes_a_tree() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for d in ["src", "tests", "examples", "templates", "script", "skills", ".github"] {
        walk(&root.join(d), &mut files);
    }
    let me = root.join("tests/no_tree_deletion.rs");
    let mut hits = Vec::new();
    for f in files.iter().filter(|f| **f != me) {
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        for (i, line) in text.lines().enumerate() {
            if let Some(w) = FORBIDDEN.iter().find(|w| line.contains(**w)) {
                hits.push(format!("{}:{}: {w}", f.strip_prefix(&root).unwrap().display(), i + 1));
            }
        }
    }
    assert!(hits.is_empty(), "recursive deletion found:\n{}", hits.join("\n"));
}
