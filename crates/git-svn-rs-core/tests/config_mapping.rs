use git_svn_rs_core::config::{SvnRemoteConfig, read_svn_remote_config};
use git_svn_rs_core::git::GitCli;
use git_svn_rs_core::mapping::{
    MappingKind, build_from_layout_args, build_single_path, build_standard_layout,
};

#[test]
fn standard_layout_uses_current_git_svn_default_refs() {
    let mappings = build_standard_layout("");

    assert_eq!(mappings.fetch[0].kind, MappingKind::Fetch);
    assert_eq!(mappings.fetch[0].svn_path, "trunk");
    assert_eq!(mappings.fetch[0].git_ref, "refs/remotes/origin/trunk");
    assert_eq!(mappings.branches[0].svn_path, "branches/*");
    assert_eq!(mappings.branches[0].git_ref, "refs/remotes/origin/*");
    assert_eq!(mappings.tags[0].svn_path, "tags/*");
    assert_eq!(mappings.tags[0].git_ref, "refs/remotes/origin/tags/*");
}

#[test]
fn prefix_is_applied_to_standard_layout_refs() {
    let mappings = build_standard_layout("svn/");

    assert_eq!(mappings.fetch[0].git_ref, "refs/remotes/svn/trunk");
    assert_eq!(mappings.branches[0].git_ref, "refs/remotes/svn/*");
    assert_eq!(mappings.tags[0].git_ref, "refs/remotes/svn/tags/*");
}

#[test]
fn single_path_tracks_git_svn_ref() {
    let mappings = build_single_path("");

    assert_eq!(mappings.fetch[0].svn_path, "");
    assert_eq!(mappings.fetch[0].git_ref, "refs/remotes/git-svn");
    assert!(mappings.branches.is_empty());
    assert!(mappings.tags.is_empty());
}

#[test]
fn custom_layout_args_override_stdlayout() {
    let branches = vec!["project/branches/*".to_string()];
    let tags = vec!["project/tags/{v1,v2}".to_string()];
    let mappings =
        build_from_layout_args(true, Some("project/main"), &branches, &tags, Some("svn/")).unwrap();

    assert_eq!(mappings.fetch[0].svn_path, "project/main");
    assert_eq!(mappings.fetch[0].git_ref, "refs/remotes/svn/trunk");
    assert_eq!(mappings.branches[0].svn_path, "project/branches/*");
    assert_eq!(mappings.branches[0].git_ref, "refs/remotes/svn/*");
    assert_eq!(mappings.tags[0].svn_path, "project/tags/{v1,v2}");
    assert_eq!(mappings.tags[0].git_ref, "refs/remotes/svn/tags/*");
}

#[test]
fn partial_custom_layout_does_not_create_an_implicit_trunk() {
    let branches = vec!["project/branches/*".to_string()];
    let branch_mappings = build_from_layout_args(false, None, &branches, &[], None).unwrap();
    assert!(branch_mappings.fetch.is_empty());
    assert_eq!(branch_mappings.branches[0].svn_path, "project/branches/*");
    assert!(branch_mappings.tags.is_empty());

    let tags = vec!["project/tags/*".to_string()];
    let tag_mappings = build_from_layout_args(false, None, &[], &tags, None).unwrap();
    assert!(tag_mappings.fetch.is_empty());
    assert!(tag_mappings.branches.is_empty());
    assert_eq!(tag_mappings.tags[0].svn_path, "project/tags/*");
}

#[test]
fn bare_branch_and_tag_paths_gain_the_frozen_wildcard() {
    let branches = vec!["project/branches".to_string()];
    let tags = vec!["project/tags".to_string()];
    let mappings = build_from_layout_args(false, None, &branches, &tags, Some("origin/")).unwrap();
    assert_eq!(mappings.branches[0].svn_path, "project/branches/*");
    assert_eq!(mappings.tags[0].svn_path, "project/tags/*");
}

#[test]
fn branch_or_tag_prefix_requires_a_trailing_slash() {
    let branches = vec!["branches".to_string()];
    let error = build_from_layout_args(false, None, &branches, &[], Some("custom")).unwrap_err();
    assert!(error.contains("trailing slash"));
}

#[test]
fn partial_stdlayout_overrides_preserve_unspecified_defaults() {
    let branches = vec!["project/branches/*".to_string()];
    let mappings = build_from_layout_args(true, None, &branches, &[], None).unwrap();
    assert_eq!(mappings.fetch[0].svn_path, "trunk");
    assert_eq!(mappings.branches[0].svn_path, "project/branches/*");
    assert_eq!(mappings.tags[0].svn_path, "tags/*");

    let tags = vec!["project/tags/*".to_string()];
    let mappings = build_from_layout_args(true, Some("main"), &[], &tags, None).unwrap();
    assert_eq!(mappings.fetch[0].svn_path, "main");
    assert_eq!(mappings.branches[0].svn_path, "branches/*");
    assert_eq!(mappings.tags[0].svn_path, "project/tags/*");
}

#[test]
fn invalid_multi_wildcard_layouts_are_rejected() {
    let branches = vec!["branches/*/teams/*".to_string()];
    let err = build_from_layout_args(false, None, &branches, &[], None).unwrap_err();
    assert!(err.contains("Only one set of wildcards"));
}

#[test]
fn serializes_svn_remote_config_keys() {
    let mappings = build_standard_layout("svn/");
    let config = SvnRemoteConfig::new("svn", "file:///repo", mappings)
        .with_ignore_paths("^vendor/")
        .with_include_paths("^(trunk|branches/main)/");

    let entries = config.to_git_config_entries();

    assert!(entries.contains(&("svn-remote.svn.url".to_string(), "file:///repo".to_string())));
    assert!(entries.contains(&(
        "svn-remote.svn.fetch".to_string(),
        "trunk:refs/remotes/svn/trunk".to_string()
    )));
    assert!(entries.contains(&(
        "svn-remote.svn.branches".to_string(),
        "branches/*:refs/remotes/svn/*".to_string()
    )));
    assert!(entries.contains(&(
        "svn-remote.svn.tags".to_string(),
        "tags/*:refs/remotes/svn/tags/*".to_string()
    )));
    assert!(entries.contains(&(
        "svn-remote.svn.ignore-paths".to_string(),
        "^vendor/".to_string()
    )));
    assert!(entries.contains(&(
        "svn-remote.svn.include-paths".to_string(),
        "^(trunk|branches/main)/".to_string()
    )));
}

#[test]
fn rejects_duplicate_single_value_remote_config() {
    let temp = tempfile::tempdir().unwrap();
    let git = GitCli::new(temp.path());
    git.init().unwrap();
    git.config_add("svn-remote.svn.url", "mock://one").unwrap();
    git.config_add("svn-remote.svn.url", "mock://two").unwrap();
    git.config_add("svn-remote.svn.authors-file", "one.txt")
        .unwrap();
    git.config_add("svn-remote.svn.authors-file", "two.txt")
        .unwrap();

    let url_error = read_svn_remote_config(&git, "svn").unwrap_err();
    assert!(url_error.contains("multiple values for svn-remote.svn.url"));
    assert!(url_error.contains("found 2"));

    git.run_for_test(["config", "--unset-all", "svn-remote.svn.url"])
        .unwrap();
    git.config_add("svn-remote.svn.url", "mock://one").unwrap();
    let authors_error = read_svn_remote_config(&git, "svn").unwrap_err();
    assert!(authors_error.contains("multiple values for svn-remote.svn.authors-file"));
}
