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

- [x] `maturin build --release` produces a `cp311-abi3` wheel for the
  current platform. If the stable ABI proves infeasible, the issue instead
  produces one wheel per Python version (3.11 to 3.14) and records why in
  ADR-0004.
- [x] `maturin sdist` produces a source distribution that builds on a
  machine with a Rust toolchain.
- [x] On Linux, the built extension has no dynamic dependency on
  `libssl` / `libcrypto` (checked with `ldd` or `auditwheel show`), and an
  HTTPS request to a public endpoint verifies its certificate.
- [x] The Linux wheel is tagged `manylinux_2_28`.
- [x] The package version is read from `crates/orchest-py/Cargo.toml`;
  `pyproject.toml` declares it as dynamic.
- [x] Package metadata declares the license `MIT OR Apache-2.0`, a README,
  the repository URL and `requires-python >= 3.11`.
- [x] Installing the built wheel into a fresh virtual environment and
  running `pytest python/tests` passes.
- [x] The eight published crates' manifests and features are unchanged,
  and `cargo test --workspace` and
  `cargo clippy --workspace --all-targets -- -D warnings` pass.

## Blocked by

- #324 (SDK packaging ADR)

## Verification (2026-10-02)

- The stable ABI needed no code changes. One `cp311-abi3` wheel per
  platform; the macOS arm64 wheel passed the 22 SDK tests on Python 3.13
  and 3.14 from fresh virtual environments.
- Linux arm64, built and tested in `quay.io/pypa/manylinux_2_28_aarch64`:
  the wheel is `cp311-abi3-manylinux_2_28_aarch64`, which `auditwheel`
  accepts. The extension links only glibc and base system libraries, with
  no `libssl` or `libcrypto`. The 22 SDK tests pass on Python 3.11.
  Through `orchest.complete`, `https://example.com` gets past TLS (HTTP
  405), while `https://self-signed.badssl.com` is rejected; curl
  confirms the host is reachable and its certificate untrusted.
- The vendored OpenSSL build needs the Perl modules `IPC::Cmd` and
  `Time::Piece` in manylinux images (noted for #327).
- The sdist builds from source into a fresh virtual environment and passes
  the 22 SDK tests. It needs `crates/orchest-provider-visual` included
  explicitly, because Cargo reads optional path dependencies' manifests.
- A Cargo version `0.2.0-rc.1` becomes `0.2.0rc1` on the PyPI side.
- Linux x86_64 is covered by the #327 CI job.
