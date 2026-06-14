use git_svn_rs_core::rev_map::{ObjectFormat, RevMap, RevMapRecord};
use tempfile::tempdir;

#[test]
fn writes_sha1_records_as_24_bytes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();

    map.append(1, "1111111111111111111111111111111111111111")
        .unwrap();
    map.append(2, "2222222222222222222222222222222222222222")
        .unwrap();

    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 48);
    assert_eq!(&bytes[0..4], &[0, 0, 0, 1]);
    assert_eq!(&bytes[24..28], &[0, 0, 0, 2]);
}

#[test]
fn gets_revision_by_binary_search() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();

    map.append(2, "2222222222222222222222222222222222222222")
        .unwrap();
    map.append(9, "9999999999999999999999999999999999999999")
        .unwrap();

    assert_eq!(
        map.get(9).unwrap(),
        Some("9999999999999999999999999999999999999999".to_string())
    );
    assert_eq!(map.get(4).unwrap(), None);
}

#[test]
fn all_zero_object_id_is_placeholder() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();

    map.append(10, "0000000000000000000000000000000000000000")
        .unwrap();

    assert_eq!(map.get(10).unwrap(), None);
    assert_eq!(map.max_revision(false).unwrap(), Some(10));
    assert_eq!(map.max_revision(true).unwrap(), None);
}

#[test]
fn reset_truncates_after_matching_revision() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();

    let oid1 = "1111111111111111111111111111111111111111";
    let oid2 = "2222222222222222222222222222222222222222";
    map.append(1, oid1).unwrap();
    map.append(2, oid2).unwrap();
    map.reset_to(1, oid1).unwrap();

    assert_eq!(std::fs::metadata(&path).unwrap().len(), 24);
    assert_eq!(map.get(2).unwrap(), None);
}

#[test]
fn sha256_records_are_36_bytes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha256).unwrap();

    map.append(
        1,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
    .unwrap();

    assert_eq!(std::fs::metadata(&path).unwrap().len(), 36);
}

#[test]
fn record_type_round_trips() {
    let record = RevMapRecord {
        revision: 5,
        object_id_hex: "5555555555555555555555555555555555555555".to_string(),
    };
    assert_eq!(record.revision, 5);
}

#[test]
fn max_revision_with_want_commit_uses_penultimate_when_last_is_zero() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();
    map.append(4, "4444444444444444444444444444444444444444")
        .unwrap();
    map.append(5, "0000000000000000000000000000000000000000")
        .unwrap();

    assert_eq!(map.max_record(true).unwrap().unwrap().revision, 4);
}

#[test]
fn detects_two_trailing_zero_records_as_inconsistent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();
    map.append(4, "0000000000000000000000000000000000000000")
        .unwrap();
    map.append(5, "0000000000000000000000000000000000000000")
        .unwrap();

    assert!(
        map.max_record(true)
            .unwrap_err()
            .contains("inconsistent .rev_map")
    );
}

#[test]
fn append_fails_when_lock_file_exists() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    std::fs::write(path.with_extension("uuid.lock"), []).unwrap();
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();

    let err = map
        .append(1, "1111111111111111111111111111111111111111")
        .unwrap_err();

    assert!(err.contains("rev_map lock exists"));
}

#[test]
fn append_rejects_out_of_order_revision() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();
    map.append(10, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .unwrap();

    let err = map
        .append(9, "9999999999999999999999999999999999999999")
        .unwrap_err();

    assert!(err.contains("out-of-order"));
    assert_eq!(
        map.get(10).unwrap(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string())
    );
}
