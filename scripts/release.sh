#!/usr/bin/env bash
set -euo pipefail

# Release script for piki-multi-ai
# Designed for worktree-based workflow where main and nightly live in separate directories.
#
# Usage:
#   ./scripts/release.sh <version>                # e.g. ./scripts/release.sh 1.2.0
#   ./scripts/release.sh --hotfix                 # patch release from main
#   ./scripts/release.sh --dry-run <version>      # simulate without changes
#   ./scripts/release.sh --dry-run --hotfix       # simulate hotfix
#   ./scripts/release.sh --yes 1.2.0              # skip confirmation prompts
#   ./scripts/release.sh --dry-run --yes --hotfix # combine flags

# ---------------------------------------------------------------------------
# Configuration — adjust these paths to match your worktree layout
# ---------------------------------------------------------------------------

MAIN_BRANCH="main"
DEV_BRANCH="nightly"
DRY_RUN=false
AUTO_YES=false

# Resolve worktree paths dynamically from `git worktree list`
resolve_worktree_path() {
    local branch="$1"
    git worktree list --porcelain \
        | awk -v b="$branch" '/^worktree /{wt=$2} /^branch refs\/heads\//{if($2=="refs/heads/"b) print wt}' \
        | head -1
}

MAIN_DIR=""
DEV_DIR=""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${CYAN}[info]${NC} $*"; }
ok()    { echo -e "${GREEN}[ok]${NC} $*"; }
warn()  { echo -e "${YELLOW}[warn]${NC} $*"; }
die()   { echo -e "${RED}[error]${NC} $*" >&2; exit 1; }
dry()   { echo -e "${YELLOW}[dry-run]${NC} $*"; }

# Run a command, or just print it in dry-run mode
run() {
    if $DRY_RUN; then
        dry "$*"
    else
        "$@"
    fi
}

confirm() {
    if $DRY_RUN || $AUTO_YES; then return; fi
    local msg="$1"
    echo -en "${YELLOW}$msg [y/N]${NC} "
    read -r answer
    [[ "$answer" =~ ^[Yy]$ ]] || die "Aborted."
}

# Remove stale index.lock if no other git process is using it
clear_lock() {
    local dir="$1"
    local git_dir
    git_dir="$(git -C "$dir" rev-parse --git-dir 2>/dev/null)" || return
    local lock="$git_dir/index.lock"
    if [[ -f "$lock" ]]; then
        # Check if any git process is still running for this dir
        if ! pgrep -f "git.*$(basename "$dir")" >/dev/null 2>&1; then
            warn "Removing stale lock: $lock"
            rm -f "$lock"
        else
            die "Lock file exists and git is still running: $lock"
        fi
    fi
}

# ---------------------------------------------------------------------------
# Preconditions
# ---------------------------------------------------------------------------

check_tools() {
    command -v gh    >/dev/null 2>&1 || die "gh CLI not found. Install: https://cli.github.com"
    command -v cargo >/dev/null 2>&1 || die "cargo not found"
    command -v npm   >/dev/null 2>&1 || die "npm not found (required to sync desktop frontend version)"
    gh auth status   >/dev/null 2>&1 || die "gh not authenticated. Run: gh auth login"
}

resolve_worktrees() {
    MAIN_DIR="$(resolve_worktree_path "$MAIN_BRANCH")"
    DEV_DIR="$(resolve_worktree_path "$DEV_BRANCH")"

    [[ -n "$MAIN_DIR" ]] || die "No worktree found for branch '$MAIN_BRANCH'. Create one with: git worktree add <path> $MAIN_BRANCH"
    [[ -n "$DEV_DIR" ]]  || die "No worktree found for branch '$DEV_BRANCH'. Create one with: git worktree add <path> $DEV_BRANCH"

    info "Worktrees:"
    info "  $MAIN_BRANCH → $MAIN_DIR"
    info "  $DEV_BRANCH  → $DEV_DIR"
}

check_clean() {
    local dir="$1" name="$2"
    if [[ -n "$(git -C "$dir" status --porcelain)" ]]; then
        if $DRY_RUN; then
            warn "Working tree '$name' ($dir) is dirty (ignored in dry-run)"
        else
            die "Working tree '$name' ($dir) is dirty. Commit or stash first."
        fi
    fi
}

# ---------------------------------------------------------------------------
# Version helpers
# ---------------------------------------------------------------------------

set_workspace_version() {
    local dir="$1" version="$2"
    sed -i "s/^version = \".*\"/version = \"$version\"/" "$dir/Cargo.toml"

    # Sync frontend version (package.json + package-lock.json) via `npm version`
    local frontend_dir="$dir/crates/desktop/frontend"
    if [[ -f "$frontend_dir/package.json" ]]; then
        (cd "$frontend_dir" && npm version "$version" --no-git-tag-version --allow-same-version >/dev/null)
    fi
}

# 1.2.0 → 1.3.0, 2.0.1 → 2.1.0
next_minor() {
    local v="$1"
    local major minor patch
    IFS='.' read -r major minor patch <<< "$v"
    echo "$major.$((minor + 1)).0"
}

# ---------------------------------------------------------------------------
# Release flow: nightly → main
# ---------------------------------------------------------------------------

release_from_nightly() {
    local version="$1"
    local tag="v$version"

    resolve_worktrees
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

    # 1. Quality gate (run in nightly worktree)
    if $DRY_RUN; then
        dry "cargo clippy --all-targets (skipped)"
        dry "cargo test (skipped)"
    else
        info "Running quality checks in $DEV_DIR..."
        (cd "$DEV_DIR" && cargo clippy --all-targets --quiet 2>&1) || die "clippy failed"
        (cd "$DEV_DIR" && cargo test --quiet 2>&1) || die "tests failed"
        ok "Quality checks passed"
    fi

    # 2. Bump version in nightly and commit
    info "Setting version to $version in $DEV_BRANCH..."
    run set_workspace_version "$DEV_DIR" "$version"
    run git -C "$DEV_DIR" add Cargo.toml crates/desktop/frontend/package.json crates/desktop/frontend/package-lock.json
    run git -C "$DEV_DIR" commit -m "chore: bump version to $version for release"
    ok "Version bumped in $DEV_BRANCH"

    # 3. Merge nightly into main (using main's worktree)
    info "Pulling latest $MAIN_BRANCH..."
    run git -C "$MAIN_DIR" pull origin "$MAIN_BRANCH"

    info "Merging $DEV_BRANCH → $MAIN_BRANCH..."
    run git -C "$MAIN_DIR" merge "$DEV_BRANCH" --no-ff -m "release: $tag"
    ok "Merged into $MAIN_BRANCH"

    # 4. Tag on main
    info "Creating tag $tag..."
    run git -C "$MAIN_DIR" tag "$tag"
    ok "Tag $tag created"

    # 5. Push main + tag (triggers release.yml)
    info "Pushing $MAIN_BRANCH + tag..."
    run git -C "$MAIN_DIR" push origin "$MAIN_BRANCH" --tags
    ok "Pushed — release CI triggered"

    # 6. Sync main back into nightly and reset to nightly version
    info "Syncing $MAIN_BRANCH back into $DEV_BRANCH..."
    clear_lock "$DEV_DIR"
    run git -C "$DEV_DIR" merge "$MAIN_BRANCH"
    local next_nightly
    next_nightly="$(next_minor "$version")-nightly"
    run set_workspace_version "$DEV_DIR" "$next_nightly"
    run git -C "$DEV_DIR" add Cargo.toml crates/desktop/frontend/package.json crates/desktop/frontend/package-lock.json
    run git -C "$DEV_DIR" commit -m "chore: bump version to $next_nightly after $tag release"
    run git -C "$DEV_DIR" push origin "$DEV_BRANCH"
    ok "Nightly synced and version reset"

    # 7. Watch CI
    if ! $DRY_RUN; then
        echo ""
        info "Watching release CI..."
        local run_id
        run_id="$(gh run list -R "$(git -C "$MAIN_DIR" remote get-url origin)" \
            --workflow release.yml --branch "$MAIN_BRANCH" --limit 1 \
            --json databaseId -q '.[0].databaseId' 2>/dev/null || true)"
        if [[ -n "$run_id" ]]; then
            info "Run: $run_id (Ctrl+C to stop watching — release continues in CI)"
            gh run watch "$run_id" --exit-status || warn "CI may still be running. Check: gh run view $run_id"
        fi
    fi

    echo ""
    ok "Release $tag complete!"
    local repo_url
    repo_url="$(gh repo view --json url -q .url 2>/dev/null || echo "https://github.com")"
    info "Release page: $repo_url/releases/tag/$tag"
}

# ---------------------------------------------------------------------------
# Hotfix flow: patch directly on main
# ---------------------------------------------------------------------------

hotfix() {
    resolve_worktrees
    check_clean "$MAIN_DIR" "$MAIN_BRANCH"

    # Determine next patch from latest tag
    local latest_tag
    latest_tag="$(git -C "$MAIN_DIR" describe --tags --abbrev=0 2>/dev/null)" || die "No tags found"
    local base_version="${latest_tag#v}"

    local major minor patch
    IFS='.' read -r major minor patch <<< "$base_version"
    patch=$((patch + 1))
    local version="$major.$minor.$patch"
    local tag="v$version"

    info "Hotfix: $latest_tag → $tag"
    echo ""

    # Show uncommitted changes or recent commits on main since last tag
    info "Changes on $MAIN_BRANCH since $latest_tag:"
    git -C "$MAIN_DIR" log --oneline "$latest_tag"..HEAD
    echo ""
    confirm "Proceed with hotfix $tag?"

    # Quality gate
    if $DRY_RUN; then
        dry "cargo clippy --all-targets (skipped)"
        dry "cargo test (skipped)"
    else
        info "Running quality checks in $MAIN_DIR..."
        (cd "$MAIN_DIR" && cargo clippy --all-targets --quiet 2>&1) || die "clippy failed"
        (cd "$MAIN_DIR" && cargo test --quiet 2>&1) || die "tests failed"
        ok "Quality checks passed"
    fi

    # Bump, commit, tag, push
    run set_workspace_version "$MAIN_DIR" "$version"
    run git -C "$MAIN_DIR" add Cargo.toml crates/desktop/frontend/package.json crates/desktop/frontend/package-lock.json
    run git -C "$MAIN_DIR" commit -m "chore: bump version to $version for hotfix"
    run git -C "$MAIN_DIR" tag "$tag"
    run git -C "$MAIN_DIR" push origin "$MAIN_BRANCH" --tags
    ok "Hotfix $tag pushed"

    # Sync to nightly
    info "Syncing hotfix to $DEV_BRANCH..."
    check_clean "$DEV_DIR" "$DEV_BRANCH"
    clear_lock "$DEV_DIR"
    run git -C "$DEV_DIR" merge "$MAIN_BRANCH" -m "merge: sync hotfix $tag to nightly"
    local next_nightly
    next_nightly="$(next_minor "$version")-nightly"
    run set_workspace_version "$DEV_DIR" "$next_nightly"
    run git -C "$DEV_DIR" add Cargo.toml crates/desktop/frontend/package.json crates/desktop/frontend/package-lock.json
    run git -C "$DEV_DIR" commit -m "chore: bump version to $next_nightly after hotfix $tag"
    run git -C "$DEV_DIR" push origin "$DEV_BRANCH"
    ok "Nightly synced"

    echo ""
    ok "Hotfix $tag complete!"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    # Parse flags
    while [[ "${1:-}" == --* ]]; do
        case "$1" in
            --dry-run) DRY_RUN=true; shift ;;
            --yes|-y)  AUTO_YES=true; shift ;;
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
