#!/usr/bin/env bash
set -euo pipefail

# Release script for piki-multi-ai.
# Bumps versions, builds, tags, and pushes main. Does NOT sync back to nightly
# or advance the nightly version — run scripts/post-release.sh for that after
# CI has finished and the release looks healthy.
#
# Usage:
#   ./scripts/release.sh <version>                # e.g. ./scripts/release.sh 1.2.0
#   ./scripts/release.sh --hotfix                 # patch release from main
#   ./scripts/release.sh --dry-run <version>      # simulate without changes
#   ./scripts/release.sh --dry-run --hotfix       # simulate hotfix
#   ./scripts/release.sh --yes 1.2.0              # skip confirmation prompts
#   ./scripts/release.sh --dry-run --yes --hotfix # combine flags

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DRY_RUN=false
AUTO_YES=false
export DRY_RUN AUTO_YES

# shellcheck source=lib/release-common.sh
source "$SCRIPT_DIR/lib/release-common.sh"

# ---------------------------------------------------------------------------
# Release flow: nightly → main
# ---------------------------------------------------------------------------

release_from_nightly() {
    local version="$1"
    local tag="v$version"

    resolve_worktrees
    fetch_tags "$DEV_DIR"
    require_tag_unused "$DEV_DIR" "$tag"
    check_clean "$DEV_DIR" "$DEV_BRANCH"
    check_clean "$MAIN_DIR" "$MAIN_BRANCH"

    info "Releasing $tag from $DEV_BRANCH → $MAIN_BRANCH"
    echo ""

    # Show what will be released
    local commit_count
    commit_count="$(git -C "$DEV_DIR" rev-list --count "$MAIN_BRANCH".."$DEV_BRANCH")"
    info "Commits to release: $commit_count"
    git -C "$DEV_DIR" log --oneline "$MAIN_BRANCH".."$DEV_BRANCH"
    echo ""
    confirm "Proceed with release $tag?"

    # 1. Quality gate (run in nightly worktree, pre-bump)
    if $DRY_RUN; then
        dry "cargo clippy --all-targets (skipped)"
        dry "cargo test (skipped)"
    else
        info "Running quality checks in $DEV_DIR..."
        (cd "$DEV_DIR" && cargo clippy --all-targets --quiet 2>&1) || die "clippy failed"
        (cd "$DEV_DIR" && cargo test --quiet 2>&1) || die "tests failed"
        ok "Quality checks passed"
    fi

    # 2. Bump version in nightly
    info "Setting version to $version in $DEV_BRANCH..."
    run set_workspace_version "$DEV_DIR" "$version"

    # 3. Post-bump: reproducible release build using the new Cargo.lock
    if $DRY_RUN; then
        dry "cargo build --release --locked (skipped)"
    else
        info "Verifying --locked release build in $DEV_DIR..."
        (cd "$DEV_DIR" && cargo build --release --locked --quiet) \
            || die "cargo build --release --locked failed after version bump"
        ok "Release build passed"
    fi

    # 4. Commit the bump
    run git -C "$DEV_DIR" add "${VERSION_FILES[@]}"
    run git -C "$DEV_DIR" commit -m "chore: bump version to $version for release"
    ok "Version bumped in $DEV_BRANCH"

    # 5. Merge nightly into main
    info "Pulling latest $MAIN_BRANCH..."
    run git -C "$MAIN_DIR" pull origin "$MAIN_BRANCH"

    info "Merging $DEV_BRANCH → $MAIN_BRANCH..."
    run git -C "$MAIN_DIR" merge "$DEV_BRANCH" --no-ff -m "release: $tag"
    ok "Merged into $MAIN_BRANCH"

    # 6. Annotated tag on main
    info "Creating annotated tag $tag..."
    run git -C "$MAIN_DIR" tag -a "$tag" -m "Release $tag"
    ok "Tag $tag created"

    # 7. Push main + tag (triggers release.yml)
    info "Pushing $MAIN_BRANCH + tag..."
    run git -C "$MAIN_DIR" push origin "$MAIN_BRANCH" --follow-tags
    ok "Pushed — release CI triggered"

    # 8. Watch CI
    if ! $DRY_RUN; then
        echo ""
        info "Watching release CI..."
        local head_sha run_id
        head_sha="$(git -C "$MAIN_DIR" rev-parse HEAD)"
        for attempt in 1 2 3 4 5; do
            run_id="$(gh run list -R "$(git -C "$MAIN_DIR" remote get-url origin)" \
                --workflow release.yml --branch "$MAIN_BRANCH" --limit 5 \
                --json databaseId,headSha \
                -q ".[] | select(.headSha==\"$head_sha\") | .databaseId" \
                2>/dev/null | head -1 || true)"
            [[ -n "$run_id" ]] && break
            info "Run not found yet (attempt $attempt/5), waiting..."
            sleep 3
        done
        if [[ -n "$run_id" ]]; then
            info "Run: $run_id (Ctrl+C to stop watching — release continues in CI)"
            gh run watch "$run_id" --exit-status || warn "CI may still be running. Check: gh run view $run_id"
        else
            warn "Could not locate release CI run for $head_sha. Check GitHub Actions manually."
        fi
    fi

    echo ""
    ok "Release $tag complete!"
    local repo_url
    repo_url="$(gh repo view --json url -q .url 2>/dev/null || echo "https://github.com")"
    info "Release page: $repo_url/releases/tag/$tag"
    echo ""
    info "Next step: run \`./scripts/post-release.sh\` to sync $MAIN_BRANCH into $DEV_BRANCH"
    info "           and advance the nightly version."
}

# ---------------------------------------------------------------------------
# Hotfix flow: patch directly on main
# ---------------------------------------------------------------------------

hotfix() {
    resolve_worktrees
    fetch_tags "$MAIN_DIR"
    check_clean "$MAIN_DIR" "$MAIN_BRANCH"

    # Determine next patch from latest semver tag (skip rolling tags like nightly-latest)
    local latest_tag
    latest_tag="$(git -C "$MAIN_DIR" describe --tags --abbrev=0 --match 'v[0-9]*.[0-9]*.[0-9]*' 2>/dev/null)" \
        || die "No semver tags found"
    local base_version="${latest_tag#v}"

    local major minor patch
    IFS='.' read -r major minor patch <<< "$base_version"
    patch=$((patch + 1))
    local version="$major.$minor.$patch"
    local tag="v$version"

    require_tag_unused "$MAIN_DIR" "$tag"

    info "Hotfix: $latest_tag → $tag"
    echo ""

    info "Changes on $MAIN_BRANCH since $latest_tag:"
    git -C "$MAIN_DIR" log --oneline "$latest_tag"..HEAD
    echo ""
    confirm "Proceed with hotfix $tag?"

    # 1. Quality gate (pre-bump)
    if $DRY_RUN; then
        dry "cargo clippy --all-targets (skipped)"
        dry "cargo test (skipped)"
    else
        info "Running quality checks in $MAIN_DIR..."
        (cd "$MAIN_DIR" && cargo clippy --all-targets --quiet 2>&1) || die "clippy failed"
        (cd "$MAIN_DIR" && cargo test --quiet 2>&1) || die "tests failed"
        ok "Quality checks passed"
    fi

    # 2. Bump
    info "Setting version to $version in $MAIN_BRANCH..."
    run set_workspace_version "$MAIN_DIR" "$version"

    # 3. Post-bump: reproducible release build
    if $DRY_RUN; then
        dry "cargo build --release --locked (skipped)"
    else
        info "Verifying --locked release build in $MAIN_DIR..."
        (cd "$MAIN_DIR" && cargo build --release --locked --quiet) \
            || die "cargo build --release --locked failed after hotfix bump"
        ok "Release build passed"
    fi

    # 4. Commit, tag, push
    run git -C "$MAIN_DIR" add "${VERSION_FILES[@]}"
    run git -C "$MAIN_DIR" commit -m "chore: bump version to $version for hotfix"
    run git -C "$MAIN_DIR" tag -a "$tag" -m "Hotfix $tag"
    run git -C "$MAIN_DIR" push origin "$MAIN_BRANCH" --follow-tags
    ok "Hotfix $tag pushed"

    echo ""
    ok "Hotfix $tag complete!"
    echo ""
    info "Next step: run \`./scripts/post-release.sh\` to sync $MAIN_BRANCH into $DEV_BRANCH"
    info "           and advance the nightly version."
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    # Parse flags
    while [[ "${1:-}" == --* ]]; do
        case "$1" in
            --dry-run) DRY_RUN=true; export DRY_RUN; shift ;;
            --yes|-y)  AUTO_YES=true; export AUTO_YES; shift ;;
            --hotfix)  break ;;  # not a flag, it's the command
            *) die "Unknown flag: $1" ;;
        esac
    done

    if $DRY_RUN; then
        info "DRY RUN — no changes will be made"
        echo ""
    fi

    check_tools

    if [[ $# -lt 1 ]]; then
        echo "Usage:"
        echo "  $0 [--dry-run] [--yes] <version>     Release from nightly (e.g. $0 1.2.0)"
        echo "  $0 [--dry-run] [--yes] --hotfix      Patch release from main"
        exit 1
    fi

    if [[ "$1" == "--hotfix" ]]; then
        hotfix
    else
        local version="$1"
        if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            die "Invalid version '$version'. Expected: X.Y.Z (e.g. 1.2.0)"
        fi
        release_from_nightly "$version"
    fi
}

main "$@"
