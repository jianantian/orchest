# Development Workflow

## Overview

```
Plan iteration/hotfix → Create all GitHub issues → One branch+worktree → One commit per issue → PR → Merge to main
```

One iteration (or hotfix) = one branch = one worktree. All issues in the iteration are developed sequentially on that branch, each as a single focused commit. The GitHub Project board updates automatically via commit messages.

---

## Step-by-Step

### 1. Plan the Iteration or Hotfix

Read the PRD to understand scope and dependency order:

```bash
cat docs/iteration/v0_10/prd.md
# or
cat docs/hotfix/2026_06_17/prd.md
```

### 2. Create All GitHub Issues First

Before writing any code, open a GitHub issue for every issue in the iteration or hotfix. This gives each issue a number for the `closes #N` commit reference.

```bash
# Create issues in order
gh issue create --repo jianantian/orchest \
  --title "feat: <issue title>" \
  --label "v0.10" \
  --body "$(cat docs/iteration/v0_10/issues/001-foo/spec.md)"

# Repeat for each issue in the iteration
# Note the assigned issue numbers — you'll use them in commit messages
```

### 3. Create a Branch and Worktree for the Iteration

One branch covers the entire iteration or hotfix. Use a worktree so you can keep main checked out elsewhere.

```bash
git checkout main && git pull

# Iteration
git worktree add .worktrees/v0_10 -b iteration/v0_10

# Hotfix
git worktree add .worktrees/hotfix-2026_06_17 -b hotfix/2026_06_17
```

Worktrees live under `.worktrees/` (gitignored), not as sibling directories — keeps them contained inside the repo root.

Branch naming:
- Iteration: `iteration/v0_10`
- Hotfix: `hotfix/YYYY_MM_DD`

### 4. Develop Each Issue — One Commit Per Issue

Work through issues in dependency order. Each issue is exactly one commit.

```bash
cd .worktrees/v0_10

# Implement issue 001
# ... make changes ...
git add -p
git commit -m "feat: <description> (closes #42)"

# Implement issue 002
# ... make changes ...
git add -p
git commit -m "feat: <description> (closes #43)"
```

Rules:
- **One commit per issue** — all changes for an issue go in a single commit
- **`closes #N` in every commit** — triggers automatic issue closing on push
- **Dependency order** — follow the order in the PRD; don't jump ahead
- If the plan and spec conflict, update the docs first before continuing

Each issue's documentation:
- `docs/iteration/<version>/issues/<NNN-slug>/spec.md` — acceptance criteria (hotfix uses same path under `docs/hotfix/<date>/issues/`)
- `docs/iteration/<version>/issues/<NNN-slug>/plan.md` — implementation steps (iterations only; hotfix issues embed the plan directly in spec.md)

### 5. Run Checks Before Merging

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/lint-check.sh
```

All four must pass. Fix any failures before merging.

### 6. Create a Pull Request

Push the branch and open a PR:

```bash
cd .worktrees/v0_10

git push -u origin iteration/v0_10

gh pr create --repo jianantian/orchest \
  --title "iteration: v0.10" \
  --base main \
  --body "Closes all issues in v0.10. See docs/iteration/v0_10/prd.md for scope."
```

### 7. Merge to Main

After the PR is reviewed and approved:

```bash
gh pr merge --merge --repo jianantian/orchest <PR-number>

# Clean up
cd ~/Develop/orchest
git checkout main && git pull
git worktree remove .worktrees/v0_10
git branch -d iteration/v0_10
```

After merge:
- Every `closes #N` commit **automatically closes its issue**
- All Project board cards **move to Done**
- The milestone progress bar advances

---

## Parallel Iterations with Worktrees

If two iterations or hotfixes are running in parallel (no shared files), use separate worktrees:

```bash
git worktree add .worktrees/v0_10        -b iteration/v0_10
git worktree add .worktrees/hotfix-0617  -b hotfix/2026_06_17

# Work in each independently
cd .worktrees/v0_10       && cargo test
cd .worktrees/hotfix-0617 && cargo test

# Open a PR for each and merge when approved
gh pr create --repo jianantian/orchest --title "iteration: v0.10" --base main --body "..."
gh pr create --repo jianantian/orchest --title "hotfix: 2026-06-17" --base main --body "..."

gh pr merge --merge --repo jianantian/orchest <PR-number-1>
gh pr merge --merge --repo jianantian/orchest <PR-number-2>

# Clean up
git worktree remove .worktrees/v0_10
git worktree remove .worktrees/hotfix-0617
git branch -d iteration/v0_10 hotfix/2026_06_17
```

Do not use parallel worktrees for iterations that share modified files — conflicts must be resolved on a single branch.

---

## Iteration Cadence

### Starting a New Iteration

Before writing any code:

1. Verify the previous iteration's milestone is 100% closed
2. Re-read the new iteration's `prd.md` to refresh scope and dependency order
3. Create all GitHub issues (Step 2 above)
4. Start with issue `001` — it sets up scaffolding everything else depends on

### Completing an Iteration

After all issues are merged:

1. Update `docs/iteration/roadmap.md` — move from **规划中** → **已完成**
2. If the iteration fills a gap in the 能力缺口全景 table, update 当前状态 accordingly
3. Commit the roadmap update (on main, directly):

```bash
git add docs/iteration/roadmap.md
git commit -m "docs: mark v0.X as completed in roadmap"
git push
```

### Archiving Completed Iterations and Hotfixes

After closeout, move the docs to `docs/archive/`:

```bash
mv docs/iteration/v0_10 docs/archive/v0_10
# or
mv docs/hotfix/2026_06_17 docs/archive/hotfix/2026_06_17

git add docs/archive/ docs/iteration/ docs/hotfix/
git commit -m "docs: archive completed v0.10 docs"
git push
```

---

## Quick Reference

```bash
# Create GitHub issues first (one per issue in the iteration)
gh issue create --repo jianantian/orchest --title "..." --label "v0.10" --body "..."

# Start an iteration
git worktree add .worktrees/v0_10 -b iteration/v0_10

# Commit each issue (one commit = one issue)
git commit -m "feat: <description> (closes #N)"

# Pre-PR checks
cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check && bash scripts/lint-check.sh

# Push and open PR
git push -u origin iteration/v0_10
gh pr create --repo jianantian/orchest --title "iteration: v0.10" --base main --body "..."

# Merge PR and clean up
gh pr merge --merge --repo jianantian/orchest <PR-number>
git checkout main && git pull
git worktree remove .worktrees/v0_10 && git branch -d iteration/v0_10

# Check milestone progress
gh api repos/jianantian/orchest/milestones --jq '.[] | "\(.title): \(.closed_issues)/\(.open_issues + .closed_issues)"'
```
