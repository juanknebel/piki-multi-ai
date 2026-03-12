use piki_core::preflight::{parse_git_version, run_preflight_checks};

#[test]
fn test_preflight_passes_on_dev_machine() {
    let result = run_preflight_checks();
    // git must be present in CI/dev environments
    assert!(
        result.errors.is_empty(),
        "preflight errors: {:?}",
        result.errors
    );
}

#[test]
fn test_git_version_parse_valid() {
    assert_eq!(parse_git_version("git version 2.43.0"), Some((2, 43)));
    assert_eq!(
        parse_git_version("git version 2.39.2.windows.1"),
        Some((2, 39))
    );
    assert_eq!(parse_git_version("git version 2.20.0\n"), Some((2, 20)));
}

#[test]
fn test_git_version_parse_invalid() {
    assert_eq!(parse_git_version("not a version"), None);
    assert_eq!(parse_git_version(""), None);
    assert_eq!(parse_git_version("git version"), None);
}
