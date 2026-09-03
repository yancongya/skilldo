#!/bin/bash
# SkillDo — 统一构建脚本
#
# 三端构建：
#   ./scripts/build.sh app          # 桌面客户端 DMG（默认，当前架构）
#   ./scripts/build.sh universal    # macOS Universal DMG（Intel + Apple Silicon）
#   ./scripts/build.sh win          # Windows NSIS 安装包
#   ./scripts/build.sh linux        # Linux AppImage + deb
#   ./scripts/build.sh cli          # 仅构建 CLI 二进制（保留 target/ 缓存）
#   ./scripts/build.sh release      # CLI 二进制 + 安装到 ~/.local/bin/skilldo
#
# 清理：
#   ./scripts/build.sh clean        # 删除编译缓存和输出目录（target/ + dist/ + output/）

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

TARGET="${1:-app}"
OUT_DIR="$PROJECT_ROOT/output"

# ─── clean ────────────────────────────────────────────────
if [[ "$TARGET" == "clean" ]]; then
  echo "▶ Cleaning build artifacts..."
  rm -rf "$PROJECT_ROOT/dist" "$PROJECT_ROOT/src-tauri/target" "$PROJECT_ROOT/output"
  echo "✓ Cleaned: dist/, src-tauri/target/, output/"
  exit 0
fi

# ─── clean_output（仅桌面端构建前清理旧产物，保留 target/）──
clean_output() {
  rm -rf "$OUT_DIR"
  mkdir -p "$OUT_DIR"
}

# ─── print_summary ────────────────────────────────────────
print_summary() {
  echo ""
  echo "========================================="
  echo " ✓ 构建完成"
  echo "========================================="
  echo ""
  echo "产物："
  ls -lh "$OUT_DIR/"
  echo ""
  if [[ "$TARGET" == "cli" ]]; then
    echo "安装到 PATH:"
    echo "  cp $OUT_DIR/skilldo ~/.local/bin/skilldo"
  fi
}

echo "========================================="
echo " SkillDo Build — target: $TARGET"
echo "========================================="

case "$TARGET" in
  # ─── CLI ──────────────────────────────────────────────────
  cli)
    echo ""
    echo "▶ Building CLI binary (release, no desktop app)..."
    cd src-tauri
    cargo build --release --bin skilldo
    cd "$PROJECT_ROOT"
    mkdir -p "$OUT_DIR"
    cp src-tauri/target/release/skilldo "$OUT_DIR/skilldo"
    chmod +x "$OUT_DIR/skilldo"
    echo "✓ CLI binary: output/skilldo"
    print_summary
    exit 0
    ;;

  # ─── CLI + install ────────────────────────────────────────
  release)
    echo ""
    echo "▶ Building CLI binary + installing to ~/.local/bin..."
    cd src-tauri
    cargo build --release --bin skilldo
    cd "$PROJECT_ROOT"
    mkdir -p "$OUT_DIR" ~/.local/bin
    cp src-tauri/target/release/skilldo "$OUT_DIR/skilldo"
    cp src-tauri/target/release/skilldo ~/.local/bin/skilldo
    chmod +x "$OUT_DIR/skilldo" ~/.local/bin/skilldo
    echo ""
    echo "✓ skilldo installed to ~/.local/bin/skilldo"
    echo "  版本: $(~/.local/bin/skilldo --version 2>/dev/null || echo 'unknown')"
    print_summary
    exit 0
    ;;

  # ─── Desktop clients ──────────────────────────────────────
  app|mac)
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
  *)
    echo "Unknown target: $TARGET" >&2
    echo "" >&2
    echo "用法: $0 [app|universal|win|msi|linux|all|cli|release|clean]" >&2
    exit 1
    ;;
esac

# ─── Desktop build ──────────────────────────────────────
clean_output

echo ""
echo "▶ Step 1/2: Building frontend..."
npm run build
echo "✓ Frontend built → dist/"

echo ""
echo "▶ Step 2/2: Building Rust + packaging..."
npx tauri build $TAURI_FLAG

# 收集最终产物到 output/（不含 .o/.rlib 等中间文件）
BUNDLE_DIR="src-tauri/target/release/bundle"

for f in "$BUNDLE_DIR/dmg/"*.dmg; do
  [ -f "$f" ] && cp "$f" "$OUT_DIR/" && echo "→ output/$(basename "$f")"
done
for d in "$BUNDLE_DIR/macos/"*.app; do
  [ -d "$d" ] && cp -R "$d" "$OUT_DIR/" && echo "→ output/$(basename "$d")"
done
for f in "$BUNDLE_DIR/nsis/"*.exe "$BUNDLE_DIR/msi/"*.msi "$BUNDLE_DIR/deb/"*.deb "$BUNDLE_DIR/appimage/"*.AppImage; do
  [ -f "$f" ] && cp "$f" "$OUT_DIR/" && echo "→ output/$(basename "$f")"
done

# 清理 .app 里多余的 CLI 二进制（Tauri 会把同 package 所有 bin 都打包进去）
APP_BUNDLE="$BUNDLE_DIR/macos/SkillDo.app/Contents/MacOS"
rm -f "$APP_BUNDLE/skilldo" 2>/dev/null && echo "→ cleaned extra CLI binary from .app"

# 清理不需要的衍生物：dist/（前端临时产物）+ bundle/（打包中间产物）
# 保留 src-tauri/target/ 让下次增量编译不用从头
rm -rf "$PROJECT_ROOT/dist" "$BUNDLE_DIR"

print_summary
