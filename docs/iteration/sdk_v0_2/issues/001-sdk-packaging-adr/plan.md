# 001 implementation plan

## Files to read

- `docs/iteration/sdk_v0_2/prd.md`
- `docs/adr/0003-release-policy.md`
- `pyproject.toml`, `package.json`

## Files to change

- `docs/adr/0004-sdk-packaging.md` (new)
- `docs/iteration/sdk_v0_2/prd.md` (only if the npm fallback name is used)

## Steps

1. Draft ADR-0004 from the PRD's decision table.
2. Ask the owner to create or confirm the `orchest` npm organization.
3. Record the outcome; if the scope is unavailable, switch the name to
   `orchest-sdk` in the ADR and the PRD.
4. Mark the ADR `Accepted`.
