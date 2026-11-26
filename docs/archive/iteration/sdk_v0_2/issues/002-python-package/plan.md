# 002 implementation plan

## Files to read

- `docs/adr/0004-sdk-packaging.md`
- `pyproject.toml`, `crates/orchest-py/Cargo.toml`, `crates/orchest-py/src/`
- `python/orchest/`, `python/tests/`

## Files to change

- `crates/orchest-py/Cargo.toml` (pyo3 `abi3-py311`; vendored OpenSSL on
  Linux)
- `crates/orchest-py/src/` (only where the stable ABI requires it)
- `pyproject.toml` (dynamic version, metadata)
- A Python package README

## Steps

1. Enable `abi3-py311` and fix what the stable ABI rejects. Stop and
   switch to per-version wheels if a required API is unavailable.
2. Add the Linux-only vendored OpenSSL dependency to the binding crate and
   confirm CA certificates are still found.
3. Make the version dynamic and add the package metadata.
4. Build the wheel and sdist, inspect the wheel's tags and dynamic
   dependencies, and run the tests from a fresh virtual environment.
