#!/bin/sh
set -eu

repo="${RTK_CODEX_HOOK_REPO:-LK4D4/rtk-codex-hook}"

path_contains() {
  case ":$PATH:" in
    *":$1:"*) return 0 ;;
    *) return 1 ;;
  esac
}

choose_install_dir() {
  if [ -n "${RTK_CODEX_HOOK_INSTALL_DIR:-}" ]; then
    printf '%s\n' "$RTK_CODEX_HOOK_INSTALL_DIR"
  elif path_contains "$HOME/.local/bin"; then
    printf '%s\n' "$HOME/.local/bin"
  elif path_contains "$HOME/bin"; then
    printf '%s\n' "$HOME/bin"
  else
    printf '%s\n' "$HOME/.local/bin"
  fi
}

shell_rc_file() {
  if [ -n "${RTK_CODEX_HOOK_SHELL_RC:-}" ]; then
    printf '%s\n' "$RTK_CODEX_HOOK_SHELL_RC"
    return
  fi

  shell_name="$(basename "${SHELL:-}")"
  if [ "$shell_name" = "zsh" ]; then
    printf '%s\n' "$HOME/.zshrc"
  elif [ "$shell_name" = "bash" ]; then
    printf '%s\n' "$HOME/.bashrc"
  elif [ -f "$HOME/.zshrc" ]; then
    printf '%s\n' "$HOME/.zshrc"
  elif [ -f "$HOME/.bashrc" ]; then
    printf '%s\n' "$HOME/.bashrc"
  else
    printf '%s\n' "$HOME/.profile"
  fi
}

ensure_path() {
  dir="$1"
  if [ "${RTK_CODEX_HOOK_NO_PATH_UPDATE:-}" = "1" ] || path_contains "$dir"; then
    return
  fi

  rc_file="$(shell_rc_file)"
  marker="# rtk-codex-hook PATH"
  if [ -f "$rc_file" ] && grep -Fqx "$marker" "$rc_file"; then
    return
  fi

  if [ -f "$rc_file" ]; then
    cp "$rc_file" "$rc_file.bak"
  else
    mkdir -p "$(dirname "$rc_file")"
    touch "$rc_file"
  fi

  {
    printf '\n%s\n' "$marker"
    printf 'case ":$PATH:" in\n'
    printf '  *":%s:"*) ;;\n' "$dir"
    printf '  *) export PATH="%s:$PATH" ;;\n' "$dir"
    printf 'esac\n'
    printf '# end rtk-codex-hook PATH\n'
  } >> "$rc_file"

  echo "Added $dir to PATH in $rc_file"
  echo "Restart your shell or source $rc_file before running rtk-codex-hook directly."
}

install_dir="$(choose_install_dir)"

os="$(uname -s)"
arch="$(uname -m)"
case "$os:$arch" in
  Linux:x86_64)
    target="x86_64-unknown-linux-musl"
    ;;
  Darwin:x86_64)
    target="x86_64-apple-darwin"
    ;;
  Darwin:arm64)
    target="aarch64-apple-darwin"
    ;;
  *)
    echo "unsupported platform: $os $arch" >&2
    exit 1
    ;;
esac

asset="rtk-codex-hook-$target.tar.gz"
download_base="${RTK_CODEX_HOOK_DOWNLOAD_BASE_URL:-https://github.com/$repo/releases/latest/download}"
url="$download_base/$asset"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$install_dir"
if [ -n "${RTK_CODEX_HOOK_DOWNLOAD_BASE_URL:-}" ]; then
  curl -LsSf "$url" -o "$tmp/$asset"
else
  curl --proto '=https' --tlsv1.2 -LsSf "$url" -o "$tmp/$asset"
fi
tar -xzf "$tmp/$asset" -C "$tmp"
binary="$(find "$tmp" -type f -name rtk-codex-hook | head -n 1)"
if [ -z "$binary" ]; then
  echo "downloaded archive did not contain rtk-codex-hook" >&2
  exit 1
fi

cp "$binary" "$install_dir/rtk-codex-hook"
chmod 0755 "$install_dir/rtk-codex-hook"
"$install_dir/rtk-codex-hook" --install-codex-hook
ensure_path "$install_dir"

echo "Installed rtk-codex-hook to $install_dir/rtk-codex-hook"
