# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

- Sync `Cargo.toml` and `Cargo.lock` to the release tag inside the release
  workflow so published binaries report the released version.
- Rework installation around Codex-managed setup: publish release archives, move
  detailed install/development guidance out of the README, remove installer
  scripts and binary-managed hook registration, and leave hook trust as an
  explicit user action.
