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
   - Read the official Codex hooks docs first:
     `https://developers.openai.com/codex/hooks`
   - Use only the current Codex hook schema from that page. Do not use Claude
     hook schema or invent a flatter shape.
   - Use `CODEX_HOME` if set; otherwise use `%USERPROFILE%\.codex` on Windows
     or `~/.codex` on Unix.
   - Create Codex home if missing.
   - For this install, edit `hooks.json`. If it exists, read it first; if
     absent, start from `{}`.
   - If `hooks.json` exists, write `hooks.json.bak` before changing it.
   - Ensure `hooks.PreToolUse` exists as an array of matcher groups. Each
     matcher group has optional `matcher` and required `hooks` array. Each
     handler in that inner `hooks` array must include `type: "command"` and
     `command`.
   - Do not set a platform-specific matcher. Codex currently reports shell-like
     commands to hooks as `Bash`, even on Windows, and other Codex surfaces may
     route shell execution through shell-like tool names. Let this hook receive
     `PreToolUse` events broadly; the binary only rewrites recognized shell-like
     command inputs and fails open for other tools.
   - Ensure there is one broad `PreToolUse` matcher group: no `matcher`, an
     empty-string matcher, or `"matcher": "*"`.
   - Add this handler to that broad group only if the exact command is absent
     from the broad group's inner `hooks` array:

     ```json
     {
       "hooks": [
         {
           "type": "command",
           "command": "<absolute installed binary path>",
            "statusMessage": "Checking RTK command rewrite"
         }
       ]
     }
     ```

   - Older installs may already contain the same command under a scoped matcher
     such as `"PowerShell"`, `"^PowerShell$"`, `"Bash"`, or `"^Bash$"`. Do not
     treat those scoped entries as satisfying the broad install. Add the handler
     to the broad group anyway, so shell-like tool names outside the scoped
     matcher still reach the binary.
   - If a scoped matcher group contains only this hook command, prefer migrating
     it by removing the `matcher` instead of adding a duplicate group. If the
     scoped group contains other handlers, leave it intact and add this handler
     to the broad group.
   - Do not add direct `hooks.PreToolUse[].command` entries.
   - The hook binary owns runtime `updatedInput` responses; `hooks.json` config
     shape does not change for command rewrites.
   - Preserve all existing hooks and unknown fields.
   - Write pretty JSON plus trailing newline.
   - Confirm `hooks.json` has `hooks.PreToolUse[].hooks[].command` equal to the
     absolute installed binary path.
5. Verify with the absolute installed binary path:
   - `<absolute installed binary path> --version`
   - `<absolute installed binary path> --explain "git status --short"` prints
     `rtk git status --short`
6. Do not edit Codex hook trust metadata directly. Tell the user:
   - Restart Codex so it reloads `hooks.json`.
   - Open `/hooks` in Codex and trust the new hook, or approve the hook trust
     prompt if Codex shows one.
