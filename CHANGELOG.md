# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

- Rewrite read-only `git -C <path> ...` commands through RTK while continuing to
  leave mutating `git -C` commands and pathspec `git diff -- ...` forms alone.
- Make installed `PreToolUse` config matcherless and preserve full shell-like
  tool inputs when returning `updatedInput`, so Windows/Desktop shell tools can
  be rewritten without dropping arguments such as `workdir`.
- Clarify that installers must migrate or add a broad matcherless hook group
  even when older scoped matcher groups already contain the same command.

## v0.1.7 - 2026-05-23

- Fail open for invalid already-RTK hook suggestions instead of suggesting
  malformed `rtk read` or `rtk grep` commands.
- Highlight Windows support in the README install guidance.

## v0.1.4 - 2026-05-18

- Make `Cargo.toml` and `Cargo.lock` the release version source of truth, and
  fail the GitHub release workflow when the tag does not match the package
  version.

## v0.1.3 - 2026-05-18

- Sync `Cargo.toml` and `Cargo.lock` to the release tag inside the release
  workflow so published binaries report the released version.
- Rework installation around Codex-managed setup: publish release archives, move
  detailed install/development guidance out of the README, remove installer
  scripts and binary-managed hook registration, and leave hook trust as an
  explicit user action.
