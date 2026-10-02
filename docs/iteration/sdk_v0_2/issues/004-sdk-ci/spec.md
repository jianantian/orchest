# 004 · Three-platform SDK CI

GitHub issue: #327

## Background

CI builds and tests the bindings on Linux x86_64 only, from a development
build. Nothing checks that release packages build and work on the other
two supported platforms, or that the stable-ABI wheel works across Python
versions.

## Goal

A CI job proves, on every supported platform, that the release packages
build, install and pass the SDK tests. The release workflow can call the
same job.

## Acceptance Criteria

- [ ] An SDK job release-builds the Python wheel and the Node addon on
  Linux x86_64, Linux arm64 and macOS arm64, each on a native runner.
- [ ] On each platform, the job installs the built wheel into a fresh
  virtual environment and runs `pytest python/tests`, and installs the
  packed npm packages into an empty directory and runs the `js/tests`
  suite.
- [ ] On Linux x86_64, the Python tests run on 3.11 and on the latest
  supported Python, and the Node tests run on Node 18 and Node 22.
- [ ] The job builds the sdist once and uploads the wheels, the sdist and
  the packed npm packages as workflow artifacts.
- [ ] The job runs on pushes to `main`, on pull requests that touch the
  binding crates, `python/`, `js/`, `pyproject.toml`, `package.json`, the
  workspace manifests or the job's own workflow file, and through
  `workflow_call`.
- [ ] A pull request that changes only documentation does not start the
  job.
- [ ] The job is green on all three platforms.

## Blocked by

- #325 (Python package)
- #326 (Node package)

## Notes

- Found in #325: the vendored OpenSSL build (OpenSSL 3.6 via
  `openssl-src`) needs the Perl modules `IPC::Cmd` and `Time::Piece`,
  which the `manylinux_2_28` images do not ship. Install
  `perl-IPC-Cmd perl-Time-Piece` before building on Linux (for example in
  `maturin-action`'s `before-script-linux`). The Node build on Linux needs
  the same packages.
