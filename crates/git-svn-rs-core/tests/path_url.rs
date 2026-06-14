use git_svn_rs_core::path_url::{add_path_to_url, canonicalize_path, canonicalize_url, join_paths};

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
