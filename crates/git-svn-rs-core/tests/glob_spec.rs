use git_svn_rs_core::glob_spec::GlobSpec;

#[test]
fn parses_single_star_glob_like_git_svn() {
    let spec = GlobSpec::new("branches/*", true).unwrap();
    assert_eq!(spec.left(), "branches");
    assert_eq!(spec.right(), "");
    assert_eq!(spec.depth(), 1);
    assert_eq!(spec.full_path("main"), "branches/main");
    assert!(spec.is_match("branches/main"));
    assert!(!spec.is_match("branches/main/nested"));
}

#[test]
fn supports_brace_pattern_when_pattern_ok() {
    let spec = GlobSpec::new("branches/{stable,release}", true).unwrap();
    assert_eq!(spec.left(), "branches");
    assert_eq!(spec.depth(), 1);
    assert!(spec.is_match("branches/stable"));
    assert!(spec.is_match("branches/release"));
    assert!(!spec.is_match("branches/main"));
}

#[test]
fn rejects_multiple_wildcard_groups() {
    let err = GlobSpec::new("branches/*/teams/*", true).unwrap_err();
    assert!(err.contains("Only one set of wildcards"));
}

#[test]
fn rejects_more_than_one_star_in_one_segment() {
    let err = GlobSpec::new("branches/**", true).unwrap_err();
    assert!(err.contains("Only one '*' is allowed"));
}
