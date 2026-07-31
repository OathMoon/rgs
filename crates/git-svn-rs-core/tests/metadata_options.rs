use git_svn_rs_core::config::MetadataOptions;

#[test]
fn rejects_no_metadata_with_svm_props() {
    let err = MetadataOptions {
        no_metadata: true,
        use_svm_props: true,
        use_svnsync_props: false,
        rewrite_root: None,
        rewrite_uuid: None,
    }
    .validate()
    .unwrap_err();
    assert!(err.contains("noMetadata"));
    assert!(err.contains("useSvmProps"));
}

#[test]
fn rejects_no_metadata_with_svnsync_props() {
    let err = MetadataOptions {
        no_metadata: true,
        use_svm_props: false,
        use_svnsync_props: true,
        rewrite_root: None,
        rewrite_uuid: None,
    }
    .validate()
    .unwrap_err();
    assert!(err.contains("noMetadata"));
    assert!(err.contains("useSvnsyncProps"));
}

#[test]
fn rejects_svm_props_with_svnsync_props() {
    let err = MetadataOptions {
        no_metadata: false,
        use_svm_props: true,
        use_svnsync_props: true,
        rewrite_root: None,
        rewrite_uuid: None,
    }
    .validate()
    .unwrap_err();
    assert!(err.contains("useSvmProps"));
    assert!(err.contains("useSvnsyncProps"));
}

#[test]
fn rejects_svm_props_with_rewrite_root() {
    let err = MetadataOptions {
        no_metadata: false,
        use_svm_props: true,
        use_svnsync_props: false,
        rewrite_root: Some("https://mirror.example".to_string()),
        rewrite_uuid: None,
    }
    .validate()
    .unwrap_err();
    assert!(err.contains("useSvmProps"));
    assert!(err.contains("rewriteRoot"));
}

#[test]
fn rejects_svm_props_with_rewrite_uuid() {
    let err = MetadataOptions {
        no_metadata: false,
        use_svm_props: true,
        use_svnsync_props: false,
        rewrite_root: None,
        rewrite_uuid: Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_string()),
    }
    .validate()
    .unwrap_err();
    assert!(err.contains("useSvmProps"));
    assert!(err.contains("rewriteUUID"));
}

#[test]
fn rejects_svnsync_props_with_rewrite_root() {
    let err = MetadataOptions {
        use_svnsync_props: true,
        rewrite_root: Some("https://mirror.example".to_string()),
        ..MetadataOptions::default()
    }
    .validate()
    .unwrap_err();
    assert!(err.contains("useSvnsyncProps"));
    assert!(err.contains("rewriteRoot"));
}

#[test]
fn rejects_svnsync_props_with_rewrite_uuid() {
    let err = MetadataOptions {
        use_svnsync_props: true,
        rewrite_uuid: Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_string()),
        ..MetadataOptions::default()
    }
    .validate()
    .unwrap_err();
    assert!(err.contains("useSvnsyncProps"));
    assert!(err.contains("rewriteUUID"));
}
