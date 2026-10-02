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

Reserved branches:
- `vintage` — previous major version backup; never deleted. Kept as a historical snapshot, not a development branch.

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
- **Changelog entry for user-visible changes**: see [Changelog](#changelog) below

Each issue's documentation:
- `docs/iteration/<version>/issues/<NNN-slug>/spec.md` — acceptance criteria (hotfix uses same path under `docs/hotfix/<date>/issues/`)
- `docs/iteration/<version>/issues/<NNN-slug>/plan.md` — implementation steps (iterations only; hotfix issues embed the plan directly in spec.md)

### Changelog

`CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
A commit adds an entry under `## [Unreleased]` in the same commit when it
changes anything a user of a published crate can observe:

- public API of a Supported crate (ADR-0003 D2), including items
  `orchest-provider` re-exports;
- Cargo features, MSRV or public dependencies;
- documented runtime behavior (event ordering, wire field names, error
  kinds, default options);
- bug fixes that users can observe.

Use the Keep a Changelog categories: Added, Changed, Deprecated, Removed,
Fixed, Security. A breaking change starts with **Breaking:**. Commits that
only touch docs, tests, CI, internal refactoring, bindings or examples need
no entry. At release time the `Unreleased` section is renamed to the new
version (ADR-0003 D7 tag format) and a fresh empty `Unreleased` section is
added above it.

### 5. Run Checks Before Merging

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
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

## Releasing

Published crates are released in lockstep from a version tag through
`.github/workflows/release.yml` ([ADR-0003](docs/adr/0003-release-policy.md)).

### One-time setup

- Create a crates.io API token with the `publish-new` and `publish-update`
  scopes, and add it as the repository secret `CARGO_REGISTRY_TOKEN`.

### Cutting a release

1. On `main`, set `[workspace.package] version` and every internal
   requirement in `[workspace.dependencies]` to the new version: a caret for
   Supported crates, `=` for Internal crates.
2. Rename `## [Unreleased]` in `CHANGELOG.md` to `## [X.Y.Z]`, add a fresh
   empty `## [Unreleased]` above it, and update the compare links.
3. Check locally:

   ```bash
   scripts/release-check.sh vX.Y.Z
   scripts/release-publish.sh X.Y.Z --dry-run
   ```

4. Optionally run the **Release** workflow manually with `dry_run` checked
   (the default). It runs the guards, the full CI suite and
   `cargo publish --workspace --dry-run`, and uploads nothing.
5. Commit, then tag and push:

   ```bash
   git tag vX.Y.Z
   git push origin main vX.Y.Z
   ```

The tag push runs the guards and the full CI suite. It then publishes the
crates in ADR-0003 D7 order and creates the GitHub Release from the
changelog section. A version with a pre-release suffix (`-rc.N`) is marked
as a prerelease.

### Recovering a failed release

Re-run the **Release** workflow manually from the same tag with `dry_run`
unchecked. Crates already on crates.io at that version are skipped, and an
existing GitHub Release is left alone. A version that was published with a
defect cannot be replaced: yank it with `cargo yank` and release a patch.

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
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check && bash scripts/lint-check.sh

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
