# Development Workflow

## Overview

```
Pick issue → In Progress → branch workwtree → develop → merge to main → issue auto-closes
```

One issue = one branch = one merge commit. No PRs for solo work. The GitHub Project board updates automatically via commit messages.

---

## Step-by-Step

### 1. Pick an Issue

Work through issues in order within each iteration — most have sequential dependencies. Check the current iteration's PRD for the dependency order.

```bash
# View open issues for v0.1
gh issue list --repo jianantian/orchest --label "v0.1" --state open

# View the milestone progress
gh api repos/jianantian/orchest/milestones --jq '.[] | {title, open_issues, closed_issues}'
```

### 2. Set to In Progress

Move the card on the [Project board](https://github.com/users/jianantian/projects/1) to **In Progress**, or do it from the CLI:

```bash
gh issue edit <N> --repo jianantian/orchest --add-label "in-progress"
```

### 3. Create a Branch

```bash
git checkout main && git pull
git checkout -b issue-<N>-<slug>
# e.g. git checkout -b issue-5-run-loop
```

Branch naming: `issue-<N>-<slug>` where slug matches the issue filename (e.g. `run-loop`, `core-types`).

### 4. Develop From the Issue Plan

Keep commits focused. Each issue is implemented from its own documentation bundle:

- `docs/iteration/<version>/prd.md` defines iteration scope and dependency order.
- `docs/iteration/<version>/issues/<NNN-slug>/spec.md` defines the issue contract and acceptance criteria.
- `docs/iteration/<version>/issues/<NNN-slug>/plan.md` defines the implementation sequence for that issue.

Use the issue's `plan.md` as the step-by-step implementation guide. Use the issue's `spec.md` and acceptance criteria as the definition of done. If the plan and spec conflict, stop and update the docs first so the plan, spec, and PRD stay consistent before implementation continues.

```bash
# Check the iteration scope, issue contract, and implementation plan
cat docs/iteration/v0_5/prd.md
cat docs/iteration/v0_5/issues/005-openrouter-adapter/spec.md
cat docs/iteration/v0_5/issues/005-openrouter-adapter/plan.md

# Commit as you go
git add -p
git commit -m "feat: implement run loop core state machine"
git commit -m "feat: add budget check at loop entry"
```

Write the `closes #N` reference in the **final** commit of the branch — this is what triggers automatic issue closing on push.

```bash
git commit -m "feat: complete agent run loop (closes #5)"
```

### 5. Run Checks Before Merging

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/lint-check.sh
```

All four must pass. Fix any failures before merging.

### 6. Merge to Main

```bash
git checkout main
git merge --no-ff issue-<N>-<slug>   # --no-ff preserves a merge commit per issue
git push
git branch -d issue-<N>-<slug>
```

After push:
- The `closes #N` commit **automatically closes the issue**
- The Project board card **moves to Done**
- The milestone progress bar advances

---

## Parallel Work with Worktrees

Use worktrees:

```bash
# Set up two parallel workspaces
git worktree add ../orchest-mcp   issue-16-mcp-stdio
git worktree add ../orchest-oai   issue-19-openai-adapter

# Work in each directory independently
cd ../orchest-mcp   && cargo test
cd ../orchest-oai   && cargo test

# Merge each when done (from the main repo directory)
cd ~/Develop/orchest
git merge --no-ff issue-16-mcp-stdio
git merge --no-ff issue-19-openai-adapter

# Clean up
git worktree remove ../orchest-mcp
git worktree remove ../orchest-oai
```

Do not use worktrees for issues that share modified files — resolve the conflict on a single branch instead.

---

## Iteration Cadence

### Starting a New Iteration

Before picking up the first issue of a new iteration:

1. Verify the previous iteration's milestone is 100% closed
2. Re-read the new iteration's `prd.md` to refresh scope and success metrics
3. Start with issue `001` — it sets up the scaffolding everything else depends on

### Completing an Iteration

After the last issue of an iteration is merged and all acceptance criteria are met:

1. Update `docs/iteration/roadmap.md` — move the iteration from **规划中** → **已完成** (add a table row under 已完成, remove the entry from 规划中)
2. If the iteration fills a gap listed in the 能力缺口全景 table, update the 当前状态 column accordingly
3. Commit the roadmap update:

```bash
git add docs/iteration/roadmap.md
git commit -m "docs: mark v0.X as completed in roadmap"
```

### Archiving Completed Iterations and Hotfixes

After an iteration or hotfix is closed (all issues merged, acceptance criteria met, roadmap updated), move its documentation directory to `docs/archive/`:

```bash
# Archive a completed iteration
mv docs/iteration/v0_5 docs/archive/v0_5

# Archive a completed hotfix
mv docs/hotfix/2026_05_26 docs/archive/hotfix/2026_05_26
```

This keeps the active `docs/iteration/` and `docs/hotfix/` trees focused on in-progress and upcoming work. Completed work remains accessible under `docs/archive/` for reference.

After archiving:

```bash
git add docs/archive/ docs/iteration/ docs/hotfix/
git commit -m "docs: archive completed v0.X docs"
```

### Dependency Order in v0.1

```
001 (workspace setup)
  └── 002 (core types)
        └── 003 (tool registry)
              └── 004 (model adapter)
                    └── 005 (run loop)
                          ├── 006 (budget guard)
                          ├── 007 (approval gate)
                          ├── 008 (async job)
                          ├── 009 (skill loading)
                          │     └── 010 (skill bundled tool)
                          └── 011 (builtin read_file)
                                └── 012 (Python SDK)
                                └── 013 (TypeScript SDK)
                                      └── 014 (e2e validation)
```

006–011 have some flexibility and can be interleaved once 005 is done.

### Dependency / Cadence in v0.2

v0.2 issues are partially parallelizable. Recommended cadence:

```
001 (MCP stdio)
  └── 002 (MCP HTTP)
        └── 003 (Tool Search Tool)

004 (OpenAI adapter)  // can run in parallel with 001/002/003 after core model interface is stable
005 (Context compaction) // can run in parallel with 004; touches run-loop/message management
006 (Webhook async tool) // after async job path is validated; avoid overlapping edits with 005 where possible
```

Suggested execution rhythm:

1. **MCP lane first**: complete 001 → 002 to unblock all transport-dependent tests.
2. **Parallel lane**: develop 004 and 005 in separate branches/worktrees.
3. **Finalize async reliability**: complete 006 after 005 merge to reduce run-loop conflicts.
4. **Iteration closeout**: run full workspace checks and one end-to-end pass for all v0.2 acceptance criteria.

---

## Quick Reference

```bash
# Start issue N
git checkout -b issue-<N>-<slug>

# Final commit (triggers auto-close)
git commit -m "feat: <description> (closes #<N>)"

# Pre-merge checks
cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check && bash scripts/lint-check.sh

# Merge and push
git checkout main && git merge --no-ff issue-<N>-<slug> && git push && git branch -d issue-<N>-<slug>

# Check milestone progress
gh api repos/jianantian/orchest/milestones --jq '.[] | "\(.title): \(.closed_issues)/\(.open_issues + .closed_issues)"'
```
