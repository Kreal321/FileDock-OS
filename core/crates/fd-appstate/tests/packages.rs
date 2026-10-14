use fd_appstate::packages;
#[test]
fn cached_package_is_encrypted_deduplicated_and_pinned_to_recorded_version() {
    let d = tempfile::tempdir().unwrap();
    let bytes = b"configuration-default-canary";
    let key = [7; 32];
    let hash = packages::store(d.path(), bytes, &key).unwrap();
    let path = packages::path(d.path(), &hash).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(!before.windows(bytes.len()).any(|w| w == bytes));
    assert_eq!(
        packages::load(d.path(), &hash, &key).unwrap().unwrap(),
        bytes
    );
    assert_eq!(packages::store(d.path(), bytes, &key).unwrap(), hash);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(packages::load(d.path(), &hash, &[8; 32]).is_err());
    assert!(packages::require_recorded(bytes, &hash, "1.0.0", "2.0.0").is_err());
    packages::require_recorded(bytes, &hash, "1.0.0", "1.0.0").unwrap();
}
