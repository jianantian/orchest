# 002 · Python package publishable

GitHub issue: #325

## Background

`orchest-py` builds only as a local development extension. It is not built
against the stable ABI, so it would need one wheel per Python version. On
Linux it links the system OpenSSL dynamically, so a prebuilt wheel would
fail to load on a machine with a different OpenSSL. `pyproject.toml`
carries its own version and has no license, README or URL metadata.

## Goal

`maturin build --release` produces a wheel and an sdist that are ready to
upload to PyPI.

## Acceptance Criteria

- [ ] `maturin build --release` produces a `cp311-abi3` wheel for the
  current platform. If the stable ABI proves infeasible, the issue instead
  produces one wheel per Python version (3.11 to 3.14) and records why in
  ADR-0004.
- [ ] `maturin sdist` produces a source distribution that builds on a
  machine with a Rust toolchain.
- [ ] On Linux, the built extension has no dynamic dependency on
  `libssl` / `libcrypto` (checked with `ldd` or `auditwheel show`), and an
  HTTPS request to a public endpoint verifies its certificate.
- [ ] The Linux wheel is tagged `manylinux_2_28`.
- [ ] The package version is read from `crates/orchest-py/Cargo.toml`;
  `pyproject.toml` declares it as dynamic.
- [ ] Package metadata declares the license `MIT OR Apache-2.0`, a README,
  the repository URL and `requires-python >= 3.11`.
- [ ] Installing the built wheel into a fresh virtual environment and
  running `pytest python/tests` passes.
- [ ] The eight published crates' manifests and features are unchanged,
  and `cargo test --workspace` and
  `cargo clippy --workspace --all-targets -- -D warnings` pass.

## Blocked by

- #324 (SDK packaging ADR)
