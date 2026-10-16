use fd_storage::mount_path;
use std::path::Path;
#[test]
fn scalar_mount_paths_never_gain_literal_outer_quotes() {
    assert_eq!(
        mount_path(Path::new("/mnt/filedock/pools/id/data")).unwrap(),
        "/mnt/filedock/pools/id/data"
    );
    for bad in ["relative", "/a\n[Mount]", "/a%t", "/a\"", "/a\\b"] {
        assert!(mount_path(Path::new(bad)).is_err(), "{bad:?}");
    }
}
#[test]
fn trim_does_not_follow_links_or_remove_recent_or_nested_data() {
    use std::time::Duration;
    let root = std::env::temp_dir().join(format!("fd-pressure-{}", std::process::id()));
    std::fs::create_dir_all(root.join("user-data")).unwrap();
    std::fs::write(root.join("user-data/keep"), b"user data").unwrap();
    std::os::unix::fs::symlink(root.join("user-data/keep"), root.join("link")).unwrap();
    std::fs::write(root.join("recent"), b"recent staging").unwrap();
    assert_eq!(
        fd_storage::pressure::trim_disposable(&root, Duration::from_secs(86400)).unwrap(),
        0
    );
    assert!(root.join("user-data/keep").exists());
    assert!(
        root.join("link")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    std::fs::remove_dir_all(root).unwrap();
}
