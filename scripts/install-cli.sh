#!/usr/bin/env bash
set -euo pipefail

REPO="${SKILLDO_REPO:-yancongya/skilldo}"
INSTALL_DIR="${SKILLDO_INSTALL_DIR:-$HOME/.local/bin}"
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) ASSET="skilldo-cli-macos-aarch64.tar.gz" ;;
  Darwin-x86_64) ASSET="skilldo-cli-macos-x86_64.tar.gz" ;;
  Linux-x86_64|Linux-amd64) ASSET="skilldo-cli-linux-x64.tar.gz" ;;
  Linux-*)
    echo "No prebuilt Linux CLI is published for $(uname -m) yet." >&2
    echo "Build it from source instead:" >&2
    echo "  git clone https://github.com/${REPO}.git && cd skilldo && npm install && ./scripts/build.sh cli" >&2
    exit 1 ;;
  *) echo "Unsupported platform: $(uname -s)-$(uname -m)." >&2; echo "Supported: macOS arm64/x86_64, Linux x86_64." >&2; exit 1 ;;
esac

BASE="${SKILLDO_DOWNLOAD_BASE:-https://github.com/${REPO}/releases/latest/download}"
RELEASES_URL="https://github.com/${REPO}/releases/latest"
API_BASE="https://api.github.com/repos/${REPO}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
CURL_OPTS=(--fail --location --silent --show-error --connect-timeout 20 --max-time 180 --retry 2)

# Resolve a release asset's GitHub API id. The release CDN can return 5xx while
# api.github.com stays reachable, so every download gets an API fallback.
api_asset_id() {
  local name="$1" json
  json=$(curl --fail --location --silent --show-error --connect-timeout 20 --max-time 60 \
    -H "User-Agent: skilldo-installer" -H "Accept: application/vnd.github+json" \
    "${API_BASE}/releases/latest") || return 1
  if command -v python3 >/dev/null 2>&1; then
    printf '%s' "$json" | python3 -c 'import json,sys; n=sys.argv[1]; print(next(a["id"] for a in json.load(sys.stdin)["assets"] if a["name"]==n))' "$name" 2>/dev/null && return 0
  fi
  if command -v jq >/dev/null 2>&1; then
    printf '%s' "$json" | jq -r --arg n "$name" '.assets[] | select(.name==$n) | .id' | head -1 | grep -E '^[0-9]+$' && return 0
  fi
  # Dependency-free fallback: split asset objects on '{' and take the first id in the matching block.
  printf '%s' "$json" | tr '{' '\n' | grep -F "\"name\":\"$name\"" | grep -o '"id":[0-9]*' | head -1 | cut -d: -f2 | grep -E '^[0-9]+$'
}

download_asset() {
  local name="$1" out="$2" url id
  id="$(api_asset_id "$name" 2>/dev/null || true)"
  for url in "${BASE}/${name}" "${API_BASE}/releases/assets/${id}"; do
    [[ -n "$url" && "$url" != "${API_BASE}/releases/assets/" ]] || continue
    if curl "${CURL_OPTS[@]}" -H "User-Agent: skilldo-installer" -H "Accept: application/octet-stream" "$url" --output "$out" && [[ -s "$out" ]]; then
      return 0
    fi
    rm -f "$out"
  done
  echo "Failed to download ${name} from the release CDN and the GitHub API asset endpoint." >&2
  echo "Download it manually from ${RELEASES_URL}" >&2
  return 1
}

compute_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "Neither sha256sum nor shasum is available; refusing to install an unverified binary." >&2
    return 1
  fi
}

if ! download_asset "$ASSET" "$TMP_DIR/$ASSET"; then
  if [[ "$ASSET" == skilldo-cli-linux-* ]]; then
    echo "The latest release has no Linux CLI asset yet (Linux builds ship with releases created from the Linux build job onward)." >&2
    echo "Build from source instead:" >&2
    echo "  git clone https://github.com/${REPO}.git && cd skilldo && npm install && ./scripts/build.sh cli" >&2
  fi
  exit 1
fi
download_asset "$ASSET.sha256" "$TMP_DIR/$ASSET.sha256"
EXPECTED="$(awk '{print $1}' "$TMP_DIR/$ASSET.sha256" | tr '[:upper:]' '[:lower:]')"
ACTUAL="$(compute_sha256 "$TMP_DIR/$ASSET" | tr '[:upper:]' '[:lower:]')"
[ "$EXPECTED" = "$ACTUAL" ] || { echo "SHA-256 verification failed. Expected $EXPECTED, got $ACTUAL." >&2; exit 1; }
tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"
mkdir -p "$INSTALL_DIR"
install -m 0755 "$TMP_DIR/skilldo" "$INSTALL_DIR/skilldo"
"$INSTALL_DIR/skilldo" --version
echo "Installed: $INSTALL_DIR/skilldo"
case ":$PATH:" in *":$INSTALL_DIR:"*) ;; *) echo "Add $INSTALL_DIR to PATH to run 'skilldo' from any directory." ;; esac
