#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if command -v skilldo >/dev/null 2>&1; then CLI="$(command -v skilldo)"; elif [ -x "$ROOT/src-tauri/target/release/skilldo" ]; then CLI="$ROOT/src-tauri/target/release/skilldo"; elif [ -x "$ROOT/src-tauri/target/debug/skilldo" ]; then CLI="$ROOT/src-tauri/target/debug/skilldo"; else echo "SkillDo CLI not found. Run: ./scripts/build.sh cli" >&2; exit 1; fi
exec "$CLI" device pull "$@"
