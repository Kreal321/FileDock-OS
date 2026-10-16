use fd_storage::{disks, system_pool::limits};
#[test]
fn limits_keep_the_stricter_ceiling_and_refuse_impossible_values() {
    let c = limits::capacity(256_000_000_000, 20_000_000_000, 0, 50, 100_000_000_000).unwrap();
    assert_eq!(c.bytes, 128_000_000_000);
    assert_eq!(
        limits::capacity(64_000_000_000, 10_000_000_000, 0, 50, 100_000_000_000)
            .unwrap()
            .bytes,
        0
    );
    assert!(limits::capacity(256_000_000_000, 0, 0, 99, 100_000_000_000).is_err());
    assert!(limits::capacity(256_000_000_000, 0, 0, 50, 1).is_err());
}
#[test]
fn inventory_marks_whole_system_disk_and_never_calls_a_partition_blank() {
    let value = serde_json::json!({"blockdevices":[{"path":"/dev/sda","type":"disk","serial":"SYSTEM","size":256000000000u64,"children":[{"path":"/dev/sda1","type":"part","mountpoints":["/"]}]},{"path":"/dev/sdb","type":"disk","serial":"DATA","size":20000000000u64}]});
    let d = disks::parse(&value).unwrap();
    assert!(d[0].is_system_disk);
    assert!(!d[0].is_blank);
    assert!(d[1].is_blank);
}
#[test]
fn erasure_requires_exact_order_and_disallows_system_or_missing_disks() {
    let d = disks::Disk {
        serial: "DATA".into(),
        device_path: "/dev/sdb".into(),
        is_blank: true,
        ..Default::default()
    };
    assert!(
        disks::select(
            std::slice::from_ref(&d),
            &["DATA".into()],
            Some("ERASE DATA")
        )
        .is_ok()
    );
    assert!(disks::select(std::slice::from_ref(&d), &["DATA".into()], Some("ERASE")).is_err());
    assert!(
        disks::select(
            &[disks::Disk {
                is_system_disk: true,
                ..d
            }],
            &["DATA".into()],
            Some("ERASE DATA")
        )
        .is_err()
    );
}
#[test]
fn single_partition_filesystem_can_be_adopted_without_losing_parent_identity() {
    let value = serde_json::json!({"blockdevices":[{"path":"/dev/sdb","type":"disk","serial":"EXISTING","size":20000000000u64,"children":[{"path":"/dev/sdb1","type":"part","fstype":"ext4","mountpoints":[null]}]}]});
    let d = disks::parse(&value).unwrap();
    assert_eq!(d[0].serial, "EXISTING");
    assert_eq!(d[0].device_path, "/dev/sdb1");
    assert_eq!(d[0].existing_filesystem, "ext4");
    assert!(!d[0].is_blank);
}
