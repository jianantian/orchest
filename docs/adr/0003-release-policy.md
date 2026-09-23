# ADR-0003: Release Policy for v1.0

**Status:** Accepted
**Date:** 2026-09-23 (accepted 2026-09-23)
**Deciders:** Orchest maintainer (emile) — decision owner
**Related:** [v1.0 PRD](../iteration/v1_0/prd.md), [#306](https://github.com/jianantian/orchest/issues/306),
[ADR-0001](./0001-provider-unification.md)

## Context

v1.0 is Orchest's first crates.io release. Nothing about publishing is
decided yet:

- No crate declares a license. `README.md` and `package.json` say
  `UNLICENSED`, and the GitHub repository is private.
- Every workspace member is at `0.1.0` with bare `path` dependencies.
- There is no versioning, MSRV or release-tag convention.

Facts gathered for this decision (2026-09-23, current `Cargo.lock`):

- **Internal dependency graph (normal deps):**
  `orchest-protocol` ← `orchest-provider-core` ← `orchest-provider-{http,stream,visual}`
  ← `orchest-provider`; `orchest` → `orchest-protocol` only (it uses
  `orchest-provider` as a dev-dependency); `orchest-storage` has no internal
  dependencies. `orchest-py`, `orchest-node` and the two demos depend on
  `orchest` + `orchest-protocol` + `orchest-provider`.
- **crates.io names:** all eight candidate crate names are unclaimed.
- **Third-party licenses:** every normal/build dependency of the candidate
  crates is permissive (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0,
  Unlicense). The only non-permissive term is `ryu`'s
  `Apache-2.0 OR BSL-1.0`, which offers Apache-2.0. No copyleft-only
  dependency.
- **MSRV floor from dependencies:** the highest `rust-version` in the
  candidate crates' resolved dependency tree is 1.86 (`icu_*` via `idna`).
  Orchest's own code has not been checked against an older toolchain.
- ADR-0001 says consumers select providers only through the
  `orchest-provider` wall and never name the weight-tier impl crates. But
  crates.io requires every dependency of a published crate to be published,
  so the impl crates must still be published.
- The wall already re-exports part of the lower crates. From
  `orchest-provider-core` it re-exports `Entry`, `Factory`, `ProviderConfig`,
  `CatalogExt`, `ModelRecord`, `ModelStatus` and `ModelFilter`. From
  `orchest-provider-http` it re-exports `create_adapter`,
  `create_adapter_from_config`, `normalize_provider_model`,
  `NormalizedProviderModel` and `ProviderRuntimeConfig`.
  `Registry::register_*` takes `Entry`, so custom provider registration is
  already a public extension point.
- `orchest-storage` is a standalone object-storage crate: Aliyun OSS and
  Tencent COS signing behind one `ObjectStore` trait. It is part of the
  asynchronous generation pipeline, where generated assets are persisted
  before provider-hosted URLs expire, and downstream products consume it.

## Decisions

Each decision lists the options considered. The decision owner accepted
the recommended option for every decision (D1–D8) on 2026-09-23.

### D1. License

| Option | Notes |
| --- | --- |
| **`MIT OR Apache-2.0`** (chosen) | Rust ecosystem default, matches most of our dependencies. Apache-2.0 adds a patent grant, and MIT keeps compatibility with GPLv2 users. |
| `Apache-2.0` | Patent grant, but GPLv2 projects cannot use it. |
| `MIT` | Simplest option, no patent grant. |

With a dual license, the repository root holds `LICENSE-MIT` and
`LICENSE-APACHE`, and each published crate packages both. `README.md`
changes to the chosen license. `package.json` stays `UNLICENSED` and
`private`, because binding packages are out of v1.0 scope.

### D2. Publish set and API tiers

| Member | Publish | Tier |
| --- | --- | --- |
| `orchest-protocol` | yes | **Supported** — covered by SemVer |
| `orchest` | yes | **Supported** |
| `orchest-provider` | yes | **Supported**, including the items it re-exports from the lower crates |
| `orchest-storage` | yes | **Supported** |
| `orchest-provider-core` | yes | **Internal**, apart from the items the wall re-exports |
| `orchest-provider-http` | yes | **Internal**, apart from the items the wall re-exports |
| `orchest-provider-stream` | yes | **Internal** |
| `orchest-provider-visual` | yes | **Internal** |
| `orchest-py`, `orchest-node` | no | binding packages, out of v1.0 |
| `briefing-desk-demo`, `research-pipeline-demo` | no | examples |

**Internal tier.** These crates are published only because
`orchest-provider` needs them on crates.io:

- `orchest-provider` depends on them with an exact `=X.Y.Z` requirement, so
  any change to them ships as a new `orchest-provider` release.
- Their crate-level docs say they are not for direct use.
- Direct dependents get **no** SemVer promise.

This keeps ADR-0001's wall intact, and it is the same pattern serde uses
for `serde_derive`.

**Re-exported items are Supported API.** Any item that `orchest-provider`
re-exports from an Internal crate is part of `orchest-provider`'s public API
and falls under D4. Changing it in the Internal crate is a breaking change
to `orchest-provider`.

Today that covers:

- from `orchest-provider-core`: the registry and catalog types listed in
  Context;
- from `orchest-provider-http`: the adapter-construction functions listed
  in Context.

Custom providers extend Orchest through `Registry::register_*` plus these
re-exported types. Reusing core's building blocks (`http`, `sse`, `ws`,
`oss`, `auth`, `retry`, `telemetry`, `pricing`, `gen`, `aliyun_asr`) by
depending on `orchest-provider-core` directly is **not** a supported
extension path in 1.0. Opening it later is an additive decision that needs
its own review.

#309 reviews the full re-exported set, keeps it to what the extension point
needs, and records the final list in this ADR.

### D3. Versioning scheme

| Option | Notes |
| --- | --- |
| **Lockstep** (chosen) | All published crates share one version from `[workspace.package]`, released together under one tag. This includes the standalone `orchest-storage`. The `=` pins in D2 are simple and one changelog covers everything. Downside: a crate can get a version bump with no changes. |
| Per-crate | Each crate is versioned independently. Needs per-crate tags, changelogs and release tooling (e.g. release-plz), which is not worth it for one maintainer. |

### D4. SemVer policy for Supported crates

The **public API** of a Supported crate is everything nameable from the
crate root that is not `#[doc(hidden)]`, plus its Cargo features and its
public dependencies. A public dependency is a third-party crate whose types
appear in public signatures or trait bounds.

Changes that need a **major** version:

- removing or renaming a public item, or changing a signature, a trait
  bound or a trait's required items;
- adding a variant or field to a public enum or struct that is not
  `#[non_exhaustive]`;
- removing a Cargo feature, or removing an item from default features;
- moving a public dependency to a new major version;
- changing documented runtime behavior that callers rely on. This covers
  event ordering and wire field names of `orchest-protocol` event types.

Changes allowed in a **minor** version:

- additive API, new features, additions to `#[non_exhaustive]` types;
- deprecations;
- an MSRV bump (see D5).

A **patch** version holds only bug fixes and documentation changes.

Before a crate's first release, #309 records its public dependency list.
That list is the set this policy applies to.

### D5. MSRV

| Option | Notes |
| --- | --- |
| **`rust-version = "1.87"`** (chosen) | Chosen as 1.86, the lowest version the current dependency tree allows, and then raised to 1.87 by #307. `orchest` uses the unsigned-integer `is_multiple_of`, which became stable in 1.87, and 1.87 is the lowest toolchain that builds every published crate with all features. CI gains an MSRV job. |
| Track latest stable | Zero maintenance, but it excludes enterprise and distro toolchains. |

**Bump policy** (chosen):

- An MSRV bump only happens in a minor release and is listed under
  *Changed* in the changelog.
- The MSRV never moves past the stable release from 6 months earlier
  (about 4 minor Rust versions).

The `resolver = "2"` in the workspace stays as it is. Declaring
`rust-version` lets downstream users with the MSRV-aware resolver pick
compatible dependency versions.

### D6. Edition

Keep **edition 2021** for 1.0. A crate's edition does not affect its
dependents, so migrating to 2024 can happen in any later minor release.
Doing it now adds churn to the release candidate.

### D7. Tags and publish order

- Tag format: `vX.Y.Z`, with pre-releases as `vX.Y.Z-rc.N`. There is one
  tag per lockstep release.
- Publish order:
  1. `orchest-protocol`, `orchest-storage` (no internal dependencies)
  2. `orchest-provider-core`
  3. `orchest-provider-http`, `orchest-provider-stream`,
     `orchest-provider-visual`
  4. `orchest-provider`
  5. `orchest`

### D8. Repository visibility

The `repository` field and docs.rs source links point at
`github.com/jianantian/orchest`, which is private. Publishing to crates.io
makes each crate's source public either way.

| Option | Notes |
| --- | --- |
| **Make the repository public at the 1.0.0 publish** (chosen) | `repository` links work and issues can be filed. Before the switch, review `docs/external/` (copied vendor API docs) and `docs/analysis` (symlink into Multivac): remove them or confirm they may be redistributed. |
| Keep private | Omit `repository` and point `homepage` at docs.rs. External users have no issue tracker. |

## Consequences

- #307 implements D1, D2, D3 and D5 in `Cargo.toml`, adds the license files,
  pins the Internal-tier crates with `=` and adds the MSRV CI job.
- #308 writes the changelog for the lockstep version, with compare links
  in the D7 tag format.
- #309 reviews the Supported tier against D4. That covers the four
  Supported crates and every item `orchest-provider` re-exports from Internal
  crates. #309 also records the final re-export list in D2 and the public
  dependency list.
- #310 publishes in the D7 order on `vX.Y.Z` tags.
- #311 carries out D8 if it is accepted.
