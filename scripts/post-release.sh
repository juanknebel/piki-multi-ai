#!/usr/bin/env bash
set -euo pipefail

# Post-release sync: merge main back into nightly and bump nightly to the next
# minor -nightly version. Run after ./scripts/release.sh has pushed a tag and
# CI looks healthy.
#
# Usage:
#   ./scripts/post-release.sh                 # auto-detects latest tag on main
#   ./scripts/post-release.sh --dry-run       # simulate without changes
#   ./scripts/post-release.sh --yes           # skip confirmation prompt
#   ./scripts/post-release.sh --force         # skip CI / release health check
#   ./scripts/post-release.sh --dry-run --yes # combine flags

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DRY_RUN=false
AUTO_YES=false
FORCE=false
export DRY_RUN AUTO_YES

# shellcheck source=lib/release-common.sh
source "$SCRIPT_DIR/lib/release-common.sh"

sync_main_to_nightly() {
    resolve_worktrees
    fetch_tags "$MAIN_DIR"
    check_clean "$MAIN_DIR" "$MAIN_BRANCH"
    check_clean "$DEV_DIR" "$DEV_BRANCH"

    # Detect the tagged version on main (semver tags only — skip rolling tags like nightly-latest)
    local latest_tag base_version
    latest_tag="$(git -C "$MAIN_DIR" describe --tags --abbrev=0 --match 'v[0-9]*.[0-9]*.[0-9]*' 2>/dev/null)" \
        || die "No semver tags found on $MAIN_BRANCH — run ./scripts/release.sh first"
    base_version="${latest_tag#v}"

    local next_nightly
    next_nightly="$(next_minor "$(strip_prerelease "$base_version")")-nightly"

    # Verify the tagged release actually succeeded in CI and was published
    # before advancing nightly. Skip with --force if CI/release state is known-good
    # via other means (e.g. manually published, CI offline).
    if $FORCE; then
        warn "Skipping CI / release health check (--force)"
    elif $DRY_RUN; then
        dry "require_release_healthy $MAIN_DIR $latest_tag"
    else
        require_release_healthy "$MAIN_DIR" "$latest_tag"
    fi

    # Skip if nightly already reflects the post-release state
    local current_nightly_version
    current_nightly_version="$(grep -m1 '^version = ' "$DEV_DIR/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')"
    if [[ "$current_nightly_version" == "$next_nightly" ]]; then
        warn "$DEV_BRANCH is already at $next_nightly — nothing to do"
        return 0
    fi

    # Show the work
    info "Post-release sync:"
    info "  Latest tag on $MAIN_BRANCH: $latest_tag"
    info "  Nightly version now:       $current_nightly_version"
    info "  Nightly version next:      $next_nightly"
    echo ""
    info "Commits on $MAIN_BRANCH missing from $DEV_BRANCH:"
    git -C "$DEV_DIR" log --oneline "$DEV_BRANCH".."$MAIN_BRANCH" || true
    echo ""
    confirm "Merge $MAIN_BRANCH → $DEV_BRANCH and bump to $next_nightly?"

    # 1. Merge main into nightly
    info "Merging $MAIN_BRANCH → $DEV_BRANCH..."
    clear_lock "$DEV_DIR"
    run git -C "$DEV_DIR" merge "$MAIN_BRANCH" --no-edit
    ok "Merged"

    # 2. Bump nightly version
    info "Setting version to $next_nightly in $DEV_BRANCH..."
    run set_workspace_version "$DEV_DIR" "$next_nightly"
    run git -C "$DEV_DIR" add "${VERSION_FILES[@]}"
    run git -C "$DEV_DIR" commit -m "chore: bump version to $next_nightly after $latest_tag release"

    # 3. Push
    info "Pushing $DEV_BRANCH..."
    run git -C "$DEV_DIR" push origin "$DEV_BRANCH"
    ok "$DEV_BRANCH synced and bumped to $next_nightly"
}

main() {
    while [[ "${1:-}" == --* ]]; do
        case "$1" in
            --dry-run) DRY_RUN=true; export DRY_RUN; shift ;;
            --yes|-y)  AUTO_YES=true; export AUTO_YES; shift ;;
            --force)   FORCE=true; shift ;;
            *) die "Unknown flag: $1" ;;
        esac
    done

    if $DRY_RUN; then
        info "DRY RUN — no changes will be made"
        echo ""
    fi

    check_tools
    sync_main_to_nightly
}

main "$@"
