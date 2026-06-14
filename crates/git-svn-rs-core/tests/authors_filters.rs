use git_svn_rs_core::authors::parse_authors_file;
use git_svn_rs_core::filters::{FilterDecision, PathFilters};

#[test]
fn parses_authors_file_lines() {
    let resolver =
        parse_authors_file("jdoe = Jane Doe <jane@example.com>\nsvc = Service <>\n").unwrap();

    assert_eq!(resolver.resolve("jdoe").unwrap().name, "Jane Doe");
    assert_eq!(resolver.resolve("jdoe").unwrap().email, "jane@example.com");
    assert_eq!(resolver.resolve("svc").unwrap().email, "");
}

#[test]
fn missing_author_returns_none() {
    let resolver = parse_authors_file("jdoe = Jane Doe <jane@example.com>\n").unwrap();
    assert!(resolver.resolve("unknown").is_none());
}

#[test]
fn filters_support_perl_style_negative_lookahead() {
    let filters = PathFilters::new(Some("^trunk/(?!vendor/)".to_string()), None).unwrap();

    assert_eq!(
        filters.decide("trunk/src/lib.rs").unwrap(),
        FilterDecision::Include
    );
    assert_eq!(
        filters.decide("trunk/vendor/lib.c").unwrap(),
        FilterDecision::Exclude
    );
}

#[test]
fn filters_always_reject_dot_git_paths() {
    let filters = PathFilters::new(None, None).unwrap();
    assert_eq!(
        filters.decide("trunk/.git/config").unwrap(),
        FilterDecision::Exclude
    );
}

#[test]
fn ignore_wins_over_include() {
    let filters = PathFilters::new(
        Some("^trunk/".to_string()),
        Some("^trunk/vendor/".to_string()),
    )
    .unwrap();
    assert_eq!(
        filters.decide("trunk/vendor/lib.c").unwrap(),
        FilterDecision::Exclude
    );
    assert_eq!(
        filters.decide("trunk/src/lib.rs").unwrap(),
        FilterDecision::Include
    );
}
