# T34 — Add Codex as AI provider

**Status:** DONE
**Phase:** 11 — Multi-Assistant Support
**Blocks:** —
**Blocked by:** T33

## Description

Add OpenAI Codex CLI (`codex`) as a new AI provider option in the sub-tabs system.

## Changes

Thanks to the extensible `AIProvider` design from T33, this requires only adding a new variant to the enum in `src/app.rs`.

## Acceptance Criteria

- [ ] Codex appears as a sub-tab alongside Claude Code and Gemini
- [ ] PTY spawns `codex` command for the Codex provider
- [ ] `cargo build` compiles
- [ ] `cargo test` passes
