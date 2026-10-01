# 009 implementation plan

## Files to read

- `docs/iteration/v1_0/prd.md`
- `docs/adr/0001-provider-unification.md`, `docs/adr/0002-protocol-provider-decoupling.md`
- Root `Cargo.toml` and every member `Cargo.toml`
- `CONVENTIONS.md`

## Files to change

- `docs/adr/0003-release-policy.md` (new)

## Steps

1. Derive the crate dependency graph and the resulting publish order.
2. Draft the ADR with option tables for license, versioning scheme and MSRV,
   each with a recommendation.
3. Review the options with the owner and record the chosen ones.
4. Mark the ADR `Accepted` with the decision owner.
