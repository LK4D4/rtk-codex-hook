# RTK Guidance For Codex

RTK saves context by filtering noisy command output before it reaches Codex.
Use it deliberately for commands that usually produce many tokens.

## Default

Prefer `rtk` for supported noisy commands:

```bash
rtk git status --short
rtk git diff -- src tests
rtk cargo test
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk npm run build
rtk pytest -q
rtk gh run view <id>
```

Do not blindly prefix every shell command. Keep shell/control/device tools raw
when RTK does not support them, or use `rtk proxy <cmd>` as an escape hatch.
Common raw tools: `pwsh`, `powershell`, `bash`, `sh`, `adb`, `sqlite3`, `jq`,
`node`, `where.exe`, `which`, direct scripts, and direct local binaries.

## Read Files

Use `rtk read` instead of `cat`, `Get-Content`, `sed -n`, `head`, `tail`, or
`nl -ba` when inspecting known files:

```bash
rtk read src/main.rs
rtk read -n src/main.rs
rtk read src/main.rs --max-lines 120
rtk read app.log --tail-lines 80
```

Real flags: `--line-numbers`/`-n`, `--max-lines`, `--tail-lines`, and
`--level none|minimal|aggressive`. Do not invent `--range`, `--line`, or
`--lines`.

## Filter Levels And Smart Reads

Use read levels intentionally. The flag is `--level <level>` or `-l <level>`;
do not use standalone `--minimal` or `--aggressive` unless RTK adds those flags.

```bash
rtk read src/main.rs --level none
rtk read src/main.rs --level minimal
rtk read -n src/main.rs --level aggressive
rtk smart src/main.rs
```

Use `none` or omit `--level` when exact text matters, especially short files,
line-sensitive code, configs, or useful file headers/comments. Add
`--max-lines` or `--tail-lines` to keep exact top/tail windows small.

Use `minimal` for compact first reads of medium or large files when comments and
blank lines are less important than code shape.

Use `aggressive` for structure scans of large unfamiliar files, then follow with
targeted `rtk grep` or exact `rtk read` windows.

Use `rtk smart <file>` for a quick heuristic summary of an unfamiliar source
file before deciding what to read next. Do not use it as a substitute for exact
code when editing or reviewing line-level behavior.

## Search And Discovery

Use `rtk grep` for content search and `rtk find` for file discovery:

```bash
rtk grep -n "pattern" src tests
rtk grep -n "pattern|other" .
rtk find .
rtk find src -name "*.rs"
```

Use `rtk find`, not `rtk rg --files` or `rtk grep --files`. If raw `rg` is
needed for a complex mode, keep it raw or use `rtk proxy rg ...`.

## Shell Wrappers

Avoid shell wrappers for simple commands:

```bash
rtk git status
rtk cargo test
```

Use a wrapper only when shell features are needed, such as environment setup,
compound commands, redirects, or here-docs. Keep the wrapper raw and put `rtk`
on the noisy inner command when safe:

```bash
pwsh -NoProfile -Command '$env:PATH="$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec'
bash -lc 'PATH="$HOME/.luarocks/bin:$PATH" rtk busted spec'
env PATH="$HOME/.luarocks/bin:$PATH" rtk luacheck --codes spec
```

For structured or already-bounded output, raw may be better than RTK:

```bash
jq . file.json
where.exe rtk
which rtk
adb devices
```

## Check Savings

```bash
rtk gain
rtk gain --history
rtk gain --failures
rtk --help
rtk proxy <cmd>
```

High-value habits from observed runs: use `rtk grep` for broad recursive
searches, `rtk read` for memory/session/source files, `rtk find` for recursive
file lists, and `rtk cargo test`/test wrappers for noisy test output.
