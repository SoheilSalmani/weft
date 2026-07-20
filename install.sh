#!/bin/sh
# Weft installer. Downloads a prebuilt `weft` binary from GitHub Releases and
# installs it onto your PATH.
#
#   curl -fsSL https://raw.githubusercontent.com/SoheilSalmani/weft/main/install.sh | sh
#
# Environment:
#   WEFT_VERSION      release tag to install (default: latest), e.g. v0.1.0
#   WEFT_INSTALL_DIR  install directory (default: $HOME/.local/bin)
set -eu

REPO="SoheilSalmani/weft"
BIN="weft"

info() { printf '%s\n' "$*"; }
err() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

# Resolve the Rust target triple from the current OS/arch.
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux)
    case "$arch" in
      x86_64 | amd64) target="x86_64-unknown-linux-gnu" ;;
      aarch64 | arm64) target="aarch64-unknown-linux-gnu" ;;
      *) err "unsupported architecture '$arch' on Linux — build from source: https://github.com/$REPO" ;;
    esac
    ;;
  Darwin)
    case "$arch" in
      x86_64) target="x86_64-apple-darwin" ;;
      arm64) target="aarch64-apple-darwin" ;;
      *) err "unsupported architecture '$arch' on macOS — build from source: https://github.com/$REPO" ;;
    esac
    ;;
  *)
    err "unsupported OS '$os' — build from source: https://github.com/$REPO"
    ;;
esac

version="${WEFT_VERSION:-latest}"
if [ "$version" = "latest" ]; then
  url="https://github.com/$REPO/releases/latest/download/${BIN}-${target}.tar.gz"
else
  url="https://github.com/$REPO/releases/download/${version}/${BIN}-${target}.tar.gz"
fi

bindir="${WEFT_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$bindir"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

info "downloading ${BIN} (${target}, ${version})"
if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$url" -o "$tmp/${BIN}.tar.gz" || err "download failed: $url"
elif command -v wget >/dev/null 2>&1; then
  wget -qO "$tmp/${BIN}.tar.gz" "$url" || err "download failed: $url"
else
  err "need curl or wget to download"
fi

tar -xzf "$tmp/${BIN}.tar.gz" -C "$tmp"
[ -f "$tmp/$BIN" ] || err "archive did not contain '$BIN'"
install -m 0755 "$tmp/$BIN" "$bindir/$BIN"

info "installed $BIN to $bindir/$BIN"
case ":$PATH:" in
  *":$bindir:"*) ;;
  *) info "note: add $bindir to your PATH (e.g. export PATH=\"$bindir:\$PATH\")" ;;
esac

"$bindir/$BIN" --version || true
