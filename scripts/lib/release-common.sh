# Shared helpers for release.sh and post-release.sh.
# Sourced, not executed — never set -e here; let each caller manage its own flags.

MAIN_BRANCH="${MAIN_BRANCH:-main}"
DEV_BRANCH="${DEV_BRANCH:-nightly}"
DRY_RUN="${DRY_RUN:-false}"
AUTO_YES="${AUTO_YES:-false}"

MAIN_DIR=""
DEV_DIR=""

# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------

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

# ---------------------------------------------------------------------------
# Preconditions
# ---------------------------------------------------------------------------

check_tools() {
    command -v gh    >/dev/null 2>&1 || die "gh CLI not found. Install: https://cli.github.com"
    command -v cargo >/dev/null 2>&1 || die "cargo not found"
    command -v npm   >/dev/null 2>&1 || die "npm not found (required to sync desktop frontend version)"
    gh auth status   >/dev/null 2>&1 || die "gh not authenticated. Run: gh auth login"
}

# Resolve worktree path for a given branch via `git worktree list`.
resolve_worktree_path() {
    local branch="$1"
    git worktree list --porcelain \
        | awk -v b="$branch" '/^worktree /{wt=$2} /^branch refs\/heads\//{if($2=="refs/heads/"b) print wt}' \
        | head -1
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

# Fetch tags from origin so local tag/version checks see remote state.
fetch_tags() {
    local dir="$1"
    info "Fetching tags from origin..."
    git -C "$dir" fetch origin --tags --quiet || warn "Fetch failed; proceeding with local state"
}

# Abort if $tag already exists locally or on origin.
require_tag_unused() {
    local dir="$1" tag="$2"
    if git -C "$dir" rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
        die "Tag $tag already exists locally"
    fi
    if git -C "$dir" ls-remote --exit-code --tags origin "$tag" >/dev/null 2>&1; then
        die "Tag $tag already exists on origin"
    fi
}

# Remove stale index.lock if no other git process is using it.
clear_lock() {
    local dir="$1"
    local git_dir
    git_dir="$(git -C "$dir" rev-parse --git-dir 2>/dev/null)" || return
    local lock="$git_dir/index.lock"
    if [[ -f "$lock" ]]; then
        if ! pgrep -f "git.*$(basename "$dir")" >/dev/null 2>&1; then
            warn "Removing stale lock: $lock"
            rm -f "$lock"
        else
            die "Lock file exists and git is still running: $lock"
        fi
    fi
}

# ---------------------------------------------------------------------------
# Version files
# ---------------------------------------------------------------------------

# Files that carry the project version and must be committed together on bump.
VERSION_FILES=(
    "Cargo.toml"
    "Cargo.lock"
    "crates/desktop/tauri.conf.json"
    "crates/desktop/frontend/package.json"
    "crates/desktop/frontend/package-lock.json"
    "crates/tui/src/ui/snapshots/piki_multi_ai__ui__tests__about_overlay.snap"
)

# Apply $version to every file listed in VERSION_FILES inside $dir.
# Each edit is validated post-hoc; any missing substitution aborts the script
# so we never leave half-bumped files behind.
set_workspace_version() {
    local dir="$1" version="$2"

    # 1. Cargo workspace version
    sed -i "s/^version = \".*\"/version = \"$version\"/" "$dir/Cargo.toml"
    grep -q "^version = \"$version\"$" "$dir/Cargo.toml" \
        || die "Cargo.toml version bump failed (pattern not found or not replaced)"

    # 2. Tauri bundle version (top-level "version" field)
    local tauri_conf="$dir/crates/desktop/tauri.conf.json"
    if [[ -f "$tauri_conf" ]]; then
        sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$version\"/" "$tauri_conf"
        grep -q "\"version\": \"$version\"" "$tauri_conf" \
            || die "tauri.conf.json version bump failed"
    fi

    # 3. Frontend version (package.json + package-lock.json) via `npm version`
    local frontend_dir="$dir/crates/desktop/frontend"
    if [[ -f "$frontend_dir/package.json" ]]; then
        (cd "$frontend_dir" && npm version "$version" --no-git-tag-version --allow-same-version >/dev/null) \
            || die "npm version $version failed in $frontend_dir"
        grep -q "\"version\": \"$version\"" "$frontend_dir/package.json" \
            || die "frontend package.json bump failed"
    fi

    # 4. About-overlay snapshot: version is rendered via env!("CARGO_PKG_VERSION")
    # at compile time, so the insta snapshot must be kept in sync manually.
    local about_snap="$dir/crates/tui/src/ui/snapshots/piki_multi_ai__ui__tests__about_overlay.snap"
    if [[ -f "$about_snap" ]]; then
        sed -i "s/piki-multi-ai v[^ ]*/piki-multi-ai v$version/" "$about_snap"
        grep -q "piki-multi-ai v$version" "$about_snap" \
            || die "about_overlay snapshot bump failed"
    fi

    # 5. Regenerate Cargo.lock so workspace crate entries match the new version.
    # Without this the lockfile stays out of sync and the next `cargo build`
    # silently rewrites it, breaking `--locked` builds and tag reproducibility.
    (cd "$dir" && cargo update --workspace --quiet) \
        || die "cargo update --workspace failed"
}

# Abort unless release.yml for $tag completed successfully AND the GitHub
# release for $tag exists. Prevents advancing nightly while the previous
# stable release is still broken or incomplete.
require_release_healthy() {
    local dir="$1" tag="$2"

    local sha
    sha="$(git -C "$dir" rev-list -n 1 "$tag" 2>/dev/null)" \
        || die "Cannot resolve sha for $tag"

    info "Checking release CI for $tag ($sha)..."

    local run_info
    run_info="$(gh run list --commit "$sha" --workflow release.yml --limit 1 \
        --json status,conclusion,databaseId,url \
        -q '.[0] | "\(.status // "")|\(.conclusion // "")|\(.databaseId // "")|\(.url // "")"' \
        2>/dev/null)" || run_info=""

    if [[ -z "$run_info" || "$run_info" == "|||" ]]; then
        die "No release.yml run found for $tag (sha $sha). Did the tag push trigger CI?"
    fi

    local run_status conclusion run_id url
    IFS='|' read -r run_status conclusion run_id url <<< "$run_info"

    if [[ "$run_status" != "completed" ]]; then
        die "Release CI for $tag is still '$run_status'. Wait for it to finish, then retry. Run: $url"
    fi
    if [[ "$conclusion" != "success" ]]; then
        die "Release CI for $tag ended with conclusion='$conclusion'. Fix and re-release before advancing nightly. Run: $url"
    fi
    ok "Release CI for $tag succeeded (run $run_id)"

    if ! gh release view "$tag" >/dev/null 2>&1; then
        die "GitHub release for $tag is not published. Wait for CI to publish it, or create it manually."
    fi
    ok "GitHub release for $tag is published"
}

# 1.2.0 → 1.3.0, 2.0.1 → 2.1.0
next_minor() {
    local v="$1"
    local major minor patch
    IFS='.' read -r major minor patch <<< "$v"
    echo "$major.$((minor + 1)).0"
}

# Strip trailing -nightly/-rc/etc. suffixes from a version string.
strip_prerelease() {
    echo "${1%%-*}"
}
