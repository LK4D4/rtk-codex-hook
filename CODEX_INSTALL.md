# Codex Install

Goal: install latest `LK4D4/rtk-codex-hook` release for this machine without a
local repo checkout.

1. Detect platform and pick release asset:
   - Windows x64: `rtk-codex-hook-x86_64-pc-windows-msvc.zip`
   - Linux x64: `rtk-codex-hook-x86_64-unknown-linux-musl.tar.gz`
   - macOS x64: `rtk-codex-hook-x86_64-apple-darwin.tar.gz`
   - macOS arm64: `rtk-codex-hook-aarch64-apple-darwin.tar.gz`
2. Download asset from latest GitHub release. Download `SHA256SUMS`; verify when
   present.
3. Extract binary to a stable per-user path:
   - Windows: `%LOCALAPPDATA%\rtk-codex-hook\bin\rtk-codex-hook.exe`
   - Unix: `~/.local/bin/rtk-codex-hook`
4. Register hook by editing Codex config:
   - Use `CODEX_HOME` if set; otherwise use `%USERPROFILE%\.codex` on Windows
     or `~/.codex` on Unix.
   - Create Codex home if missing.
   - Read `hooks.json` if present; if absent, start from `{}`.
   - If `hooks.json` exists, write `hooks.json.bak` before changing it.
   - Ensure `hooks.PreToolUse` exists as an array.
   - Add `{ "command": "<absolute installed binary path>" }` only if that exact
     command is absent.
   - Preserve all existing hooks and unknown fields.
   - Write pretty JSON plus trailing newline.
   - Confirm `hooks.json` has `hooks.PreToolUse[].command` equal to absolute
     installed binary path.
5. Verify with the absolute installed binary path:
   - `<absolute installed binary path> --version`
   - `<absolute installed binary path> --explain "git status --short"` prints
     `rtk git status --short`
6. Do not edit Codex hook trust metadata directly. Tell the user:
   - Restart Codex so it reloads `hooks.json`.
   - Open `/hooks` in Codex and trust the new hook, or approve the hook trust
     prompt if Codex shows one.
