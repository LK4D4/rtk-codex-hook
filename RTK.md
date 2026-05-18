# RTK Guidance For Codex

RTK trims noisy command output before it enters context. Use it for supported
commands likely to print many tokens.

## Default

Prefer `rtk` for noisy supported tools:

```bash
rtk git status --short
rtk git diff -- src tests
rtk cargo test
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk npm run build
rtk pytest -q
rtk gh run view <id>
```

Do not prefix everything. Keep shell/control/device tools raw when unsupported,
or use `rtk proxy <cmd>` as escape hatch.

Common raw tools: `pwsh`, `powershell`, `bash`, `sh`, `adb`, `sqlite3`, `jq`,
`node`, `where.exe`, `which`, direct scripts, direct local binaries.

## Read Files

Use `rtk read` for known-file inspection instead of `cat`, `Get-Content`,
`sed -n`, `head`, `tail`, or `nl -ba`:

```bash
rtk read src/main.rs
rtk read -n src/main.rs
rtk read src/main.rs --max-lines 120
rtk read app.log --tail-lines 80
```

Valid flags: `--line-numbers`/`-n`, `--max-lines`, `--tail-lines`,
`--level none|minimal|aggressive`. Do not invent `--range`, `--line`, or
`--lines`.

## Read Levels

Use `--level <level>` or `-l <level>`.

```bash
rtk read src/main.rs --level none
rtk read src/main.rs --level minimal
rtk read -n src/main.rs --level aggressive
rtk smart src/main.rs
```

Use `none` or omit `--level` when exact text matters: short files,
line-sensitive code, configs, headers, comments. Add `--max-lines` or
`--tail-lines` to bound exact reads.

Use `minimal` for compact first reads of medium/large files when comments and
blank lines matter less than code shape.

Use `aggressive` for structure scans of large unfamiliar files. Then follow
with targeted `rtk grep` or exact `rtk read` windows.

Use `rtk smart <file>` for quick heuristic source summaries. Do not rely on it
for exact code when editing or reviewing line-level behavior.

## Search And Discovery

Use `rtk grep` for content search.

```bash
rtk grep -n "pattern" src tests
rtk grep -n "pattern|other" .
```

Use `rtk find` for simple file discovery, but prefer its RTK syntax:

```bash
rtk find "*" .
rtk find "*.rs" src --max 20 --file-type f
rtk find "README.md" . --max 10
rtk find ".env*" . --max 10
```

`rtk find` supports two parsers. Native-find mode accepts only a small subset:
one effective `-name`/`-iname`, `-type`, and `-maxdepth`. RTK flags such as
`--max` work only in RTK syntax. Do not use full `find` expressions with
`rtk find`; `-not`, `!`, `-o`, `-exec`, `-print0`, size/time predicates, and
multiple meaningful `-name` branches are unsupported.

Use raw `find` for full find semantics. Use raw `rg --files` or `rtk proxy rg
...` for ripgrep-specific modes.

## Shell Wrappers

Avoid wrappers for simple commands:

```bash
rtk git status
rtk cargo test
```

Use wrappers only for shell features: env setup, compound commands, redirects,
or here-docs. Keep wrapper raw; put `rtk` on noisy inner command when safe:

```bash
pwsh -NoProfile -Command '$env:PATH="$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec'
bash -lc 'PATH="$HOME/.luarocks/bin:$PATH" rtk busted spec'
env PATH="$HOME/.luarocks/bin:$PATH" rtk luacheck --codes spec
```

Keep bounded or structured output raw when clearer:

```bash
jq . file.json
where.exe rtk
which rtk
adb devices
```
