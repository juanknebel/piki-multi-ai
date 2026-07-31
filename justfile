# Development tasks. Install with: cargo install just
#
# `just ci` runs exactly what .github/workflows/nightly.yml runs, so a green
# run here means a green run there. Keep the two in step when either changes.

# List available recipes
default:
    @just --list

# Everything CI checks, in CI order. Run before pushing to nightly.
ci: fmt-check lint test frontend

# Formatting check (CI fails on any diff)
fmt-check:
    cargo fmt --all -- --check

# Reformat in place
fmt:
    cargo fmt --all

# Clippy with warnings denied. Excludes piki-desktop, which is built by the
# build-desktop job instead (it needs the frontend dist/ to exist).
lint:
    cargo clippy --workspace --exclude piki-desktop --all-targets -- -D warnings

# Rust test suite
test:
    cargo test --workspace --exclude piki-desktop

# Typecheck + build the desktop frontend (tsc && vite build)
frontend:
    cd crates/desktop/frontend && npm run build

# Desktop Rust: needs `just frontend` first (tauri-build reads frontend/dist)
lint-desktop: frontend
    cargo clippy -p piki-desktop --all-targets -- -D warnings

# Run the TUI
run:
    cargo run -p agent-multi

# Update insta snapshots after an intentional UI change
snapshots:
    cargo insta review
