use git_svn_rs_core::path_url::{
    SvnUrlProfile, add_path_to_url, canonicalize_path, canonicalize_url, join_paths,
    repository_relative_url_path, svn_url_profile, validate_dcommit_write_urls, validate_fetch_url,
};

#[test]
fn canonicalize_path_collapses_dotdot_and_slashes() {
    assert_eq!(
        canonicalize_path("./trunk/../branches//main/"),
        "branches/main"
    );
}

#[test]
fn canonicalize_url_preserves_scheme_and_host() {
    assert_eq!(
        canonicalize_url("https://svn.example/repo/./trunk/../branches/main/"),
        "https://svn.example/repo/branches/main"
    );
}

#[test]
fn joins_non_empty_path_segments() {
    assert_eq!(join_paths(["", "trunk", "src", ""]), "trunk/src");
}

#[test]
fn adds_path_to_url_without_double_slashes() {
    assert_eq!(
        add_path_to_url("https://svn.example/repo/", "/trunk"),
        "https://svn.example/repo/trunk"
    );
}

#[test]
fn derives_decoded_repository_relative_url_paths() {
    assert_eq!(
        repository_relative_url_path(
            "file:///repo",
            "file:///repo/project%20name/%E5%88%86%E6%94%AF"
        )
        .unwrap(),
        "project name/分支"
    );
    assert!(
        repository_relative_url_path("file:///repo", "file:///other/trunk")
            .unwrap_err()
            .contains("outside repository root")
    );
    assert!(
        repository_relative_url_path("svn://one/repo", "svn://two/repo/trunk")
            .unwrap_err()
            .contains("outside repository root")
    );
}

#[test]
fn remote_protocol_profiles_match_validated_read_boundaries() {
    assert_eq!(svn_url_profile("file:///repo"), SvnUrlProfile::File);
    assert_eq!(svn_url_profile("svn://host/repo"), SvnUrlProfile::Svn);
    assert_eq!(svn_url_profile("HTTP://host/repo"), SvnUrlProfile::Http);
    assert_eq!(svn_url_profile("HTTPS://host/repo"), SvnUrlProfile::Https);
    assert!(validate_fetch_url("file:///repo").is_ok());
    assert!(validate_fetch_url("svn://host/repo").is_ok());
    assert!(validate_fetch_url("svn+ssh://host/repo").is_ok());
    assert!(validate_fetch_url("http://host/repo").is_ok());
    assert!(validate_fetch_url("https://host/repo").is_ok());
    assert!(validate_dcommit_write_urls("https://host/repo", "https://host/repo").is_ok());
    assert!(
        validate_dcommit_write_urls("https://host/repo", "http://host/repo")
            .unwrap_err()
            .contains("matching tracked")
    );
    assert!(
        validate_dcommit_write_urls("svn+ssh://host/repo/trunk", "svn+ssh://host/repo").is_ok()
    );
    assert!(
        validate_dcommit_write_urls("svn+ssh://host/repo", "file:///repo")
            .unwrap_err()
            .contains("matching tracked")
    );
}
