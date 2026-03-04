# AGENTS.md — Instructions for AI agents

## Mandatory workflow

### Tasks before code

**ALWAYS** before generating or modifying code, create the corresponding task in `claude-todo/`:

1. Create the file `claude-todo/T<NN>-<slug>.md` following the existing format (see any `T*.md` as reference)
2. Update `claude-todo/INDEX.md` with the new task in the corresponding phase
3. Mark the task as `IN_PROGRESS` before starting to write code
4. Mark as `DONE` only when the code compiles and tests pass
5. Update MEMORY.md with progress

### Documentation updates

**ALWAYS** update documentation when making changes:

1. Update `README.md` if the change affects user-facing behavior, CLI usage, architecture, or project structure
2. Update `CLAUDE.md` if the change affects build commands, architecture descriptions, or developer workflow
3. Update inline code comments only where logic is not self-evident
4. Documentation must be updated in the **same task** as the code change — never leave it for later

### Task file format

```markdown
# T<NN> — Descriptive title

**Status:** OPEN | IN_PROGRESS | DONE | CANCEL
**Phase:** N — Phase name
**Blocks:** T<XX>, T<YY> (or —)
**Blocked by:** T<XX>, T<YY> (or —)

## Description
...

## Acceptance Criteria
- [ ] Criterion 1
- [ ] Criterion 2
```
