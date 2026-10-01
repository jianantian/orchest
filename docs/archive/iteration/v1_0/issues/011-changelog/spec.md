# 011 · Changelog

GitHub issue: #308

## Background

There is no changelog. v1.0 users need a summary of what the first release
contains, and later releases need a place to record changes that the release
workflow can quote.

## Goal

Add a root `CHANGELOG.md` with the 1.0.0 entry and a documented practice for
keeping it current.

## Acceptance Criteria

- [x] `CHANGELOG.md` follows Keep a Changelog 1.1.0 with an `Unreleased`
  section and a `1.0.0` section.
- [x] The `1.0.0` section summarizes the user-visible capabilities of each
  published crate, grouped by Keep a Changelog categories, and links to the
  roadmap rather than listing every pre-1.0 issue.
- [x] Version compare links use the tag format from the release policy ADR.
- [x] `WORKFLOW.md` states when a commit must add an `Unreleased` entry.

## Blocked by

- #306 (release policy)
