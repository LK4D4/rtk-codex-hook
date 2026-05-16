#!/bin/sh
set -eu

repo="${RTK_CODEX_HOOK_REPO:-LK4D4/rtk-codex-hook}"
install_dir="${RTK_CODEX_HOOK_INSTALL_DIR:-${CARGO_HOME:-$HOME/.cargo}/bin}"

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
url="https://github.com/$repo/releases/latest/download/$asset"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$install_dir"
curl --proto '=https' --tlsv1.2 -LsSf "$url" -o "$tmp/$asset"
tar -xzf "$tmp/$asset" -C "$tmp"
binary="$(find "$tmp" -type f -name rtk-codex-hook | head -n 1)"
if [ -z "$binary" ]; then
  echo "downloaded archive did not contain rtk-codex-hook" >&2
  exit 1
fi

cp "$binary" "$install_dir/rtk-codex-hook"
chmod 0755 "$install_dir/rtk-codex-hook"
"$install_dir/rtk-codex-hook" --install-codex-hook

echo "Installed rtk-codex-hook to $install_dir/rtk-codex-hook"
