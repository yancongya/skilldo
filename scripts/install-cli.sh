#!/usr/bin/env bash
set -euo pipefail

REPO="${SKILLDO_REPO:-yancongya/skilldo}"
INSTALL_DIR="${SKILLDO_INSTALL_DIR:-$HOME/.local/bin}"
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) ASSET="skilldo-cli-macos-aarch64.tar.gz" ;;
  Darwin-x86_64) ASSET="skilldo-cli-macos-x86_64.tar.gz" ;;
  *) echo "Unsupported platform. This installer currently supports macOS arm64 and x86_64." >&2; exit 1 ;;
esac

BASE="${SKILLDO_DOWNLOAD_BASE:-https://github.com/${REPO}/releases/latest/download}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
curl --fail --location --silent --show-error "$BASE/$ASSET" --output "$TMP_DIR/$ASSET"
curl --fail --location --silent --show-error "$BASE/$ASSET.sha256" --output "$TMP_DIR/$ASSET.sha256"
EXPECTED="$(awk '{print $1}' "$TMP_DIR/$ASSET.sha256")"
ACTUAL="$(shasum -a 256 "$TMP_DIR/$ASSET" | awk '{print $1}')"
[ "$EXPECTED" = "$ACTUAL" ] || { echo "SHA-256 verification failed." >&2; exit 1; }
tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"
mkdir -p "$INSTALL_DIR"
install -m 0755 "$TMP_DIR/skilldo" "$INSTALL_DIR/skilldo"
"$INSTALL_DIR/skilldo" --version
echo "Installed: $INSTALL_DIR/skilldo"
case ":$PATH:" in *":$INSTALL_DIR:"*) ;; *) echo "Add $INSTALL_DIR to PATH to run 'skilldo' from any directory." ;; esac
