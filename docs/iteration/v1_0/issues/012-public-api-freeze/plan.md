# 012 implementation plan

## Files to read

- `docs/adr/0003-release-policy.md`
- `docs/polaris/concept-boundaries.md`, `CONVENTIONS.md`
- `lib.rs` of each published crate

## Files to change

- `docs/review/v1_0_public_api.md` (new) plus per-crate inventories
- Published crates' sources (visibility, `#[non_exhaustive]`)
- Bindings and examples affected by visibility changes

## Steps

1. Generate the per-crate public API inventory and record the command.
2. Flag growth-prone types, accidental exports and exposed third-party types.
3. Present the proposed changes for owner review.
4. Apply the approved changes and fix affected call sites.
5. Regenerate the inventory and record per-crate approval.
