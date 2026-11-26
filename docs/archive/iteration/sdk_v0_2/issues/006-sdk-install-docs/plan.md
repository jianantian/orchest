# 006 implementation plan

## Files to read

- `docs/adr/0004-sdk-packaging.md`
- `README.md`, `docs/guide/sdk-python.md`, `docs/guide/sdk-typescript.md`
- The Python package README from issue 002

## Files to change

- `README.md`
- `docs/guide/sdk-python.md`, `docs/guide/sdk-typescript.md`
- The Python package README

## Steps

1. Rewrite the install sections around the published packages and move the
   source build into a contributor section.
2. Add the platform and minimum-version tables and the unsupported-platform
   behavior.
3. Add the `orchest` import-name note and the SDK versioning note.
4. Check every command against the package names in ADR-0004.
