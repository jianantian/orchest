# Issue 005 Plan: Omni evidence report

## Files to Read

- `docs/iteration/v0_9_11/prd.md`
- All v0.9.11 issue specs and plans
- `docs/todo/provider-unification.md`
- Provider decision output from Issue 001
- Manual-run docs, test output and limitation notes from issues 002-004

## Files to Change

- `docs/iteration/v0_9_11/evidence.md`
- Optional: `docs/todo/provider-unification.md` if Step 2 assumptions changed
- Optional: PRD or issue docs if validation reveals scope corrections

## Steps

1. Run fake/unit checks for the implemented realtime path.
2. Run the manual live provider command when credentials are available.
3. Capture exact commands, environment variable names, provider/model, date and outcomes.
4. Write observed facts separately from provider-specific quirks and proposed abstraction changes.
5. List refactor inputs for provider-core/unification and identify evidence gaps.
6. Update the todo only if the existing Step 2 direction is materially changed.
7. Run or document required final checks from the PRD.
