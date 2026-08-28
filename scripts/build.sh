#!/bin/bash
# Skills Hub — 构建脚本
# 用法:
#   ./scripts/build.sh              # macOS DMG（当前架构）
#   ./scripts/build.sh universal    # macOS Universal DMG（Intel + Apple Silicon）
#   ./scripts/build.sh win          # Windows NSIS 安装包
#   ./scripts/build.sh linux        # Linux AppImage + deb
#   ./scripts/build.sh all          # 当前平台全部格式
#   ./scripts/build.sh cli          # 仅构建 CLI 二进制（不打包桌面应用）
#   ./scripts/build.sh release      # 构建 release CLI 到 ~/.local/bin/skillhub

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

TARGET="${1:-dmg}"
TAURI_FLAG=""

echo "========================================="
echo " Skills Hub Build — target: $TARGET"
echo "========================================="

# 1. 前端构建（tsc + vite）
echo ""
echo "▶ Step 1/2: Building frontend..."
npm run build
echo "✓ Frontend built → dist/"

# 2. Rust 构建 + 打包
echo ""
echo "▶ Step 2/2: Building Rust + packaging..."
case "$TARGET" in
  dmg|mac)
    TAURI_FLAG="--bundles dmg"
    ;;
  universal)
    TAURI_FLAG="--target universal-apple-darwin --bundles dmg"
    ;;
  win|nsis)
    TAURI_FLAG="--bundles nsis"
    ;;
  msi)
    TAURI_FLAG="--bundles msi"
    ;;
  linux)
    TAURI_FLAG="--bundles deb,appimage"
    ;;
  all)
    TAURI_FLAG="--bundles dmg"
    ;;
  cli)
    echo "▶ Building CLI binary only (no desktop app)..."
    cd src-tauri
    cargo build --release --bin skillhub
    echo ""
    echo "✓ CLI binary: src-tauri/target/release/skillhub"
    echo ""
    echo "安装到 PATH:"
    echo "  cp src-tauri/target/release/skillhub ~/.local/bin/skillhub"
    exit 0
    ;;
  release)
    echo "▶ Building release CLI + installing to ~/.local/bin..."
    cd src-tauri
    cargo build --release --bin skillhub
    mkdir -p ~/.local/bin
    cp target/release/skillhub ~/.local/bin/skillhub
    chmod +x ~/.local/bin/skillhub
    echo ""
    echo "✓ skillhub installed to ~/.local/bin/skillhub"
    echo "  版本: $(~/.local/bin/skillhub --version 2>/dev/null || echo 'unknown')"
    exit 0
    ;;
  *)
    echo "Unknown target: $TARGET"
    echo "用法: $0 [dmg|universal|win|msi|linux|all|cli|release]"
    exit 1
    ;;
esac

npx tauri build $TAURI_FLAG

# 输出结果路径
echo ""
echo "========================================="
echo " ✓ 构建完成"
echo "========================================="
echo ""

BUNDLE_DIR="src-tauri/target/release/bundle"
if [ -d "$BUNDLE_DIR/dmg" ]; then
  echo "macOS DMG:"
  ls -1 "$BUNDLE_DIR/dmg/"*.dmg 2>/dev/null || true
fi
if [ -d "$BUNDLE_DIR/macos" ]; then
  echo "macOS App:"
  ls -1d "$BUNDLE_DIR/macos/"*.app 2>/dev/null || true
fi
if [ -d "$BUNDLE_DIR/nsis" ]; then
  echo "Windows NSIS:"
  ls -1 "$BUNDLE_DIR/nsis/"*.exe 2>/dev/null || true
fi
if [ -d "$BUNDLE_DIR/deb" ]; then
  echo "Linux deb:"
  ls -1 "$BUNDLE_DIR/deb/"*.deb 2>/dev/null || true
fi
if [ -d "$BUNDLE_DIR/appimage" ]; then
  echo "Linux AppImage:"
  ls -1 "$BUNDLE_DIR/appimage/"*.AppImage 2>/dev/null || true
fi
echo ""
echo "CLI 二进制:"
ls -lh src-tauri/target/release/skillhub 2>/dev/null || true
