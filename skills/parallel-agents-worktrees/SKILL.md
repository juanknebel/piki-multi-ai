---
name: parallel-agents-worktrees
license: MIT
description: Run several AI coding agents in parallel on one repo, one git worktree per task, then review and merge. Use when the user wants to split work across agents, parallelize tasks, or isolate an agent's changes from the main checkout.
---

# Parallel agents with git worktrees

One task → one branch → one worktree → one agent. Agents never share a working directory.

## 1. Split the work

- List candidate tasks. Keep only those that touch **disjoint files or modules**.
- Tasks that edit the same files run **sequentially**, not in parallel.
- Each task needs: a title, a one-paragraph goal, acceptance criteria (tests/commands that must pass).
- 2–4 parallel agents is the practical ceiling; more means more review than coding.

## 2. Create worktrees

Keep worktrees **outside** the repo so tools that walk the tree don't see them.

```bash
REPO=$(git rev-parse --show-toplevel)
NAME=$(basename "$REPO")
BASE=${BASE:-$(git symbolic-ref --short HEAD)}   # branch to fork from
ROOT=${WT_ROOT:-$HOME/.worktrees/$NAME}

git fetch origin "$BASE"
git worktree add -b feature/<slug> "$ROOT/<slug>" "origin/$BASE"
```

Branch prefix by task type:

| Type | Prefix |
|---|---|
| New behavior | `feature/` |
| Defect | `bug/` |
| Investigation / throwaway | `spike/` |

Use the same `<slug>` for branch and directory.

## 3. Prepare each worktree

Untracked files are **not** copied. Per worktree, as needed:

- Copy local config: `.env`, `config.local.*` (never commit them).
- Install deps: `npm ci`, `uv sync`, etc.
- **Ports:** give each worktree its own dev-server/DB port (e.g. `PORT=$((3000 + n))`).
- **Build caches:** a shared cache dir (e.g. one Cargo `target/`) saves disk, but anything that bakes absolute paths into binaries will point at the wrong worktree. When in doubt, use a per-worktree cache or clean the affected package before testing.
- **Databases / sockets / lock files:** isolate per worktree if the app writes to a fixed path.

## 4. Launch one agent per worktree

Start each agent **with its cwd set to the worktree**. Prompt template:

```
Task: <title>
Goal: <one paragraph>
Scope: only files under <paths>. Do not touch <paths owned by other tasks>.
Done when: <commands that must pass, e.g. test + lint>.
Plan first, then implement. Commit on this branch with focused commits. Do not push or merge.
```

Rules:
- One agent per worktree. Never point two agents at the same directory.
- Put role or project instructions in the repo's agent files (`AGENTS.md`, `CLAUDE.md`, subagent files) so every worktree inherits them.
- Run agents in a multiplexer (tmux, zellij, or a dedicated orchestrator) so they survive a closed terminal.

## 5. Monitor

- Check each agent for: waiting on permission, idle, finished, error.
- Answer permission prompts promptly; a blocked agent costs wall-clock time.
- If an agent drifts outside its scope, stop it and tighten the prompt rather than patching by hand.

## 6. Review and merge

For each finished branch:

```bash
cd "$ROOT/<slug>"
git fetch origin "$BASE" && git rebase "origin/$BASE"
<run the acceptance commands>
git diff "origin/$BASE"...HEAD --stat
```

- Review the diff as you would a human PR. Reject scope creep.
- Merge in dependency order; rebase the remaining branches after each merge.
- On conflict: resolve in the later branch's worktree, rerun its checks.

## 7. Clean up

```bash
git worktree remove "$ROOT/<slug>"
git branch -d feature/<slug>        # -D only if intentionally discarding
git worktree prune
```

## Pitfalls

- `git worktree add` fails if the branch is already checked out elsewhere — one branch per worktree.
- Submodules are not initialized in new worktrees: `git submodule update --init`.
- Hooks and `git config` are shared across worktrees (same `.git`).
- Long-lived worktrees drift; rebase daily or keep tasks short.

## Tooling

This workflow is what [piki-multi-ai](https://github.com/juanknebel/piki-multi-ai) automates: worktree creation per task, branch naming from a kanban card, agent launch with a composed prompt, live agent status (running / needs permission / idle / done), persistent sessions, and review/merge — in a TUI or a desktop app.
