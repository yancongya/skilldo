#!/bin/bash
# SkillDo — 构建脚本
# 用法:
#   ./scripts/build.sh              # macOS DMG（当前架构）
#   ./scripts/build.sh universal    # macOS Universal DMG（Intel + Apple Silicon）
#   ./scripts/build.sh win          # Windows NSIS 安装包
#   ./scripts/build.sh linux        # Linux AppImage + deb
#   ./scripts/build.sh all          # 当前平台全部格式
#   ./scripts/build.sh cli          # 仅构建 CLI 二进制（不打包桌面应用）
#   ./scripts/build.sh release      # 构建 release CLI 到 ~/.local/bin/skilldo

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

TARGET="${1:-dmg}"
TAURI_FLAG=""

echo "========================================="
echo " SkillDo Build — target: $TARGET"
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
    cargo build --release --bin skilldo
    echo ""
    echo "✓ CLI binary: src-tauri/target/release/skilldo"
    echo ""
    echo "安装到 PATH:"
    echo "  cp src-tauri/target/release/skilldo ~/.local/bin/skilldo"
    exit 0
    ;;
  release)
    echo "▶ Building release CLI + installing to ~/.local/bin..."
    cd src-tauri
    cargo build --release --bin skilldo
    mkdir -p ~/.local/bin
    cp target/release/skilldo ~/.local/bin/skilldo
    chmod +x ~/.local/bin/skilldo
    echo ""
    echo "✓ skilldo installed to ~/.local/bin/skilldo"
    echo "  版本: $(~/.local/bin/skilldo --version 2>/dev/null || echo 'unknown')"
    exit 0
    ;;
  *)
    echo "Unknown target: $TARGET"
    echo "用法: $0 [dmg|universal|win|msi|linux|all|cli|release]"
    exit 1
    ;;
esac

npx tauri build $TAURI_FLAG

# 收集最终产物到 out/
OUT_DIR="$PROJECT_ROOT/out"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

BUNDLE_DIR="src-tauri/target/release/bundle"

# 复制 DMG
for f in "$BUNDLE_DIR/dmg/"*.dmg; do
  [ -f "$f" ] && cp "$f" "$OUT_DIR/" && echo "→ out/$(basename "$f")"
done

# 复制 .app
for d in "$BUNDLE_DIR/macos/"*.app; do
  [ -d "$d" ] && cp -R "$d" "$OUT_DIR/" && echo "→ out/$(basename "$d")"
done

# 复制 Windows/Linux 产物
for f in "$BUNDLE_DIR/nsis/"*.exe "$BUNDLE_DIR/msi/"*.msi "$BUNDLE_DIR/deb/"*.deb "$BUNDLE_DIR/appimage/"*.AppImage; do
  [ -f "$f" ] && cp "$f" "$OUT_DIR/" && echo "→ out/$(basename "$f")"
done

# 复制 CLI 二进制
cp src-tauri/target/release/skilldo "$OUT_DIR/" && echo "→ out/skilldo"

# 输出结果
echo ""
echo "========================================="
echo " ✓ 构建完成"
echo "========================================="
echo ""
echo "所有产物已复制到 out/:"
ls -lh "$OUT_DIR/"

# 清理构建衍生品（保留 src-tauri/target/ 缓存以加速下次构建）
echo ""
echo "▶ Cleaning build intermediates..."
rm -rf "$BUNDLE_DIR/dmg"/*.dmg "$BUNDLE_DIR/dmg"/*.sh \
       "$BUNDLE_DIR/dmg"/*.icns 2>/dev/null || true
rm -rf "$BUNDLE_DIR/nsis" "$BUNDLE_DIR/msi" "$BUNDLE_DIR/deb" "$BUNDLE_DIR/appimage" 2>/dev/null || true
rm -rf "$BUNDLE_DIR/macos" 2>/dev/null || true
echo "✓ Intermediate bundles cleaned"
