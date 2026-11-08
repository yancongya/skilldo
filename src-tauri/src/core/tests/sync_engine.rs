use std::fs;

use crate::core::sync_engine::{
    copy_dir_recursive, is_same_link, link_mode_at, sync_dir_for_tool_with_overwrite,
    sync_dir_hybrid, sync_dir_hybrid_with_overwrite, SyncMode,
};

#[test]
fn copy_dir_recursive_skips_git_dir() {
    let src_dir = tempfile::tempdir().unwrap();
    let dst_dir = tempfile::tempdir().unwrap();

    fs::create_dir_all(src_dir.path().join(".git")).unwrap();
    fs::create_dir_all(src_dir.path().join("sub")).unwrap();
    fs::write(src_dir.path().join("sub/a.txt"), b"ok").unwrap();
    fs::write(src_dir.path().join(".git/secret"), b"no").unwrap();

    copy_dir_recursive(src_dir.path(), dst_dir.path()).unwrap();
    assert!(dst_dir.path().join("sub/a.txt").exists());
    assert!(!dst_dir.path().join(".git").exists());
}

#[test]
fn hybrid_sync_creates_link_and_is_idempotent_when_same_link() {
    let src_dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(src_dir.path().join("s")).unwrap();
    fs::write(src_dir.path().join("s/a.txt"), b"ok").unwrap();

    let dst_dir = tempfile::tempdir().unwrap();
    let target = dst_dir.path().join("t");

    let out = sync_dir_hybrid(src_dir.path(), &target).unwrap();
    assert!(matches!(
        out.mode_used,
        SyncMode::Symlink | SyncMode::Junction | SyncMode::Copy
    ));

    // On Windows a junction stores its target as an NT path (`\\?\C:\...`),
    // so comparing read_link() byte-for-byte with the source is wrong.
    if link_mode_at(&target).is_some() {
        assert!(is_same_link(&target, src_dir.path()));
        let out2 = sync_dir_hybrid(src_dir.path(), &target).unwrap();
        assert!(matches!(
            out2.mode_used,
            SyncMode::Symlink | SyncMode::Junction
        ));
        assert!(!out2.replaced);
        assert!(is_same_link(&target, src_dir.path()));
    }
}

#[test]
fn hybrid_sync_with_overwrite_replaces_existing() {
    let src_dir = tempfile::tempdir().unwrap();
    fs::write(src_dir.path().join("a.txt"), b"src").unwrap();

    let dst_dir = tempfile::tempdir().unwrap();
    let target = dst_dir.path().join("t");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("old.txt"), b"old").unwrap();

    let err = sync_dir_hybrid_with_overwrite(src_dir.path(), &target, false).unwrap_err();
    assert!(format!("{:#}", err).contains("target already exists"));

    let out = sync_dir_hybrid_with_overwrite(src_dir.path(), &target, true).unwrap();
    assert!(out.replaced);
}

#[test]
fn cursor_sync_forces_copy() {
    let src_dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(src_dir.path().join("s")).unwrap();
    fs::write(src_dir.path().join("s/a.txt"), b"ok").unwrap();

    let dst_dir = tempfile::tempdir().unwrap();
    let target = dst_dir.path().join("t");

    let out = sync_dir_for_tool_with_overwrite("cursor", src_dir.path(), &target, false).unwrap();
    assert!(matches!(out.mode_used, SyncMode::Copy));
    assert!(target.join("s/a.txt").exists());
    assert_eq!(fs::read(target.join("s/a.txt")).unwrap(), b"ok");
}

#[cfg(unix)]
#[test]
fn copy_overwrite_replaces_broken_symlink_target() {
    use std::os::unix::fs::symlink;

    let src_dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(src_dir.path().join("s")).unwrap();
    fs::write(src_dir.path().join("s/a.txt"), b"ok").unwrap();

    let dst_dir = tempfile::tempdir().unwrap();
    let target = dst_dir.path().join("t");

    // Create a broken symlink at the target path.
    symlink(dst_dir.path().join("missing"), &target).unwrap();

    let out = crate::core::sync_engine::sync_dir_copy_with_overwrite(src_dir.path(), &target, true)
        .unwrap();

    assert!(matches!(out.mode_used, SyncMode::Copy));
    assert!(target.join("s/a.txt").exists());
    assert_eq!(fs::read(target.join("s/a.txt")).unwrap(), b"ok");
}

#[cfg(windows)]
mod windows_links {
    use super::*;
    use crate::core::sync_engine::remove_path_any;

    fn junction_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("central").join("my-skill");
        let link = root.path().join("agent").join("my-skill");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        fs::write(source.join("SKILL.md"), b"central content").unwrap();
        junction::create(&source, &link).unwrap();
        (root, source, link)
    }

    /// Regression: `\\?\C:\...` (junction) vs `C:\...` (DB central_path).
    /// A byte comparison used to report "different", so `sync` deleted the
    /// junction and replaced it with a full copy on every run.
    #[test]
    fn verbatim_prefix_junction_is_recognized_and_left_alone() {
        let (_root, source, link) = junction_fixture();

        assert!(is_same_link(&link, &source));
        assert!(matches!(link_mode_at(&link), Some(SyncMode::Junction)));

        // Windows paths are case-insensitive.
        let upper = std::path::PathBuf::from(source.to_string_lossy().to_uppercase());
        assert!(is_same_link(&link, &upper));

        let out = sync_dir_hybrid_with_overwrite(&source, &link, true).unwrap();
        assert!(!out.replaced);
        assert!(matches!(out.mode_used, SyncMode::Junction));
        assert!(is_same_link(&link, &source));
        assert_eq!(
            fs::read(source.join("SKILL.md")).unwrap(),
            b"central content"
        );

        let out2 = sync_dir_hybrid(&source, &link).unwrap();
        assert!(!out2.replaced);
        assert!(matches!(out2.mode_used, SyncMode::Junction));
        assert!(is_same_link(&link, &source));
    }

    /// A junction is a directory reparse point, not a symlink, so the old
    /// `remove_dir_all` path could walk into the central repo.
    #[test]
    fn removing_a_junction_never_touches_the_source() {
        let (_root, source, link) = junction_fixture();

        remove_path_any(&link).unwrap();
        assert!(fs::symlink_metadata(&link).is_err());
        assert_eq!(
            fs::read(source.join("SKILL.md")).unwrap(),
            b"central content"
        );
    }

    /// A successful sync must report the mode actually used, otherwise the
    /// database records `symlink` for what is really a junction.
    #[test]
    fn hybrid_sync_reports_the_real_link_mode() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("central");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("a.txt"), b"ok").unwrap();
        let target = root.path().join("agent").join("skill");

        let out = sync_dir_hybrid(&source, &target).unwrap();
        assert!(
            matches!(out.mode_used, SyncMode::Junction | SyncMode::Symlink),
            "unexpected silent copy fallback: {:?}",
            out.mode_used
        );
        assert!(is_same_link(&target, &source));
    }
}
