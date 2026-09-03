#!/usr/bin/env bash
# SkillDo — 交互式安装脚本
# 用法: curl -fsSL https://raw.githubusercontent.com/yancongya/skilldo/main/scripts/install.sh | bash
set -euo pipefail

REPO="${SKILLDO_REPO:-yancongya/skilldo}"
REleases_URL="https://github.com/${REPO}/releases/latest"
INSTALL_DIR="${SKILLDO_INSTALL_DIR:-$HOME/.local/bin}"

# ─── 颜色 ─────────────────────────────────────────────────
R='\033[0;31m' G='\033[0;32m' Y='\033[0;33m' B='\033[0;34m'
C='\033[0;36m' DIM='\033[2m' BOLD='\033[1m' RST='\033[0m'

# ─── 检测平台 ─────────────────────────────────────────────
detect_platform() {
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)   echo "macos-aarch64" ;;
    Darwin-x86_64)  echo "macos-x86_64" ;;
    Linux-x86_64)   echo "linux-x64" ;;
    Linux-aarch64)  echo "linux-aarch64" ;;
    MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
    *) echo "unknown" ;;
  esac
}

PLATFORM=$(detect_platform)

# ─── 检测已安装状态 ────────────────────────────────────────
check_installed() {
  local found=0
  if command -v skilldo &>/dev/null; then
    local ver
    ver=$(skilldo --version 2>/dev/null | head -1)
    echo -e "  ${G}✓${RST} CLI: ${BOLD}skilldo${RST} ${DIM}($ver)${RST}"
    found=1
  fi
  # 检测桌面端
  local app_path=""
  case "$PLATFORM" in
    macos-*) app_path="/Applications/SkillDo.app" ;;
    windows) app_path="$LOCALAPPDATA/Programs/SkillDo" ;;
  esac
  if [[ -n "$app_path" && -e "$app_path" ]]; then
    echo -e "  ${G}✓${RST} Desktop: ${BOLD}SkillDo.app${RST} ${DIM}($app_path)${RST}"
    found=1
  fi
  if [[ $found -eq 0 ]]; then
    echo -e "  ${DIM}(未检测到已安装的 SkillDo)${RST}"
  fi
}

# ─── 安装 CLI ──────────────────────────────────────────────
install_cli() {
  echo ""
  echo -e "${BOLD}▶ 安装 CLI${RST}"

  if [[ "$PLATFORM" == "unknown" ]]; then
    echo -e "  ${R}✗ 不支持的平台: $(uname -s)-$(uname -m)${RST}" >&2
    echo -e "  ${DIM}支持: macOS arm64/x86_64, Linux x86_64/aarch64, Windows${RST}" >&2
    return 1
  fi

  if [[ "$PLATFORM" == "windows" ]]; then
    echo -e "  ${Y}→ Windows 平台，请在 PowerShell 中运行:${RST}"
    echo -e "  ${C}irm ${REleases_URL}/download/skilldo-cli-windows-x64.zip -OutFile skilldo.zip${RST}"
    echo -e "  ${DIM}或直接下载: ${REleases_URL}${RST}"
    return 0
  fi

  local asset="skilldo-cli-${PLATFORM}.tar.gz"
  local base="${SKILLDO_DOWNLOAD_BASE:-https://github.com/${REPO}/releases/latest/download}"
  local tmp_dir
  tmp_dir=$(mktemp -d)
  trap 'rm -rf "$tmp_dir"' RETURN

  echo -e "  ${DIM}→ 下载 ${asset}...${RST}"
  if ! curl --fail --location --silent --show-error "$base/$asset" -o "$tmp_dir/$asset" 2>/dev/null; then
    echo -e "  ${R}✗ 下载失败。请检查网络或手动下载:${RST}" >&2
    echo -e "  ${C}${REleases_URL}${RST}" >&2
    return 1
  fi

  echo -e "  ${DIM}→ 校验 SHA-256...${RST}"
  if ! curl --fail --location --silent --show-error "$base/$asset.sha256" -o "$tmp_dir/$asset.sha256" 2>/dev/null; then
    echo -e "  ${Y}⚠ 无法下载校验文件，跳过校验${RST}"
  else
    local expected actual
    expected=$(awk '{print $1}' "$tmp_dir/$asset.sha256")
    actual=$(shasum -a 256 "$tmp_dir/$asset" 2>/dev/null | awk '{print $1}')
    if [[ "$expected" != "$actual" ]]; then
      echo -e "  ${R}✗ SHA-256 校验失败${RST}" >&2
      return 1
    fi
    echo -e "  ${G}✓ 校验通过${RST}"
  fi

  echo -e "  ${DIM}→ 安装到 ${INSTALL_DIR}...${RST}"
  mkdir -p "$INSTALL_DIR"
  tar -xzf "$tmp_dir/$asset" -C "$tmp_dir"
  install -m 0755 "$tmp_dir/skilldo" "$INSTALL_DIR/skilldo"

  local ver
  ver=$("$INSTALL_DIR/skilldo" --version 2>/dev/null | head -1 || echo "unknown")
  echo -e "  ${G}✓${RST} ${BOLD}skilldo${RST} ${DIM}$ver${RST} 已安装到 ${BOLD}$INSTALL_DIR/skilldo${RST}"

  # PATH 提示
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) echo -e "\n  ${Y}⚠ 将 $INSTALL_DIR 加入 PATH:${RST}"
       echo -e "  ${C}echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc && source ~/.zshrc${RST}" ;;
  esac

  # 快速体验
  echo ""
  echo -e "  ${BOLD}快速体验:${RST}"
  echo -e "  ${C}skilldo list --json${RST}        ${DIM}# 查看已管理的 skills${RST}"
  echo -e "  ${C}skilldo status${RST}             ${DIM}# 检测已安装的 AI 工具${RST}"
  echo -e "  ${C}skilldo explore --query ai${RST} ${DIM}# 浏览 skill 市场${RST}"
}

# ─── 安装桌面端 ────────────────────────────────────────────
install_desktop() {
  echo ""
  echo -e "${BOLD}▶ 桌面客户端${RST}"

  case "$PLATFORM" in
    macos-*)
      echo -e "  ${DIM}→ 正在打开 GitHub Releases 页面...${RST}"
      open "$REleases_URL" 2>/dev/null || echo -e "  ${C}$REleases_URL${RST}"
      echo -e "  ${G}✓${RST} 下载 ${BOLD}.dmg${RST} 文件，拖入 Applications 即可"
      echo -e "  ${DIM}支持自动更新：启动后检查新版本并提示安装${RST}"
      ;;
    windows)
      echo -e "  ${DIM}→ 正在打开 GitHub Releases 页面...${RST}"
      start "$REleases_URL" 2>/dev/null || echo -e "  ${C}$REleases_URL${RST}"
      echo -e "  ${G}✓${RST} 下载 ${BOLD}.exe${RST} 安装包运行即可"
      echo -e "  ${DIM}支持 NSIS 自动更新${RST}"
      ;;
    linux)
      echo -e "  ${DIM}→ Linux 桌面端需要从源码构建:${RST}"
      echo -e "  ${C}git clone https://github.com/${REPO}.git && cd skilldo && npm install && npm run tauri:dev${RST}"
      echo -e "  ${DIM}或下载 CLI 版本（推荐）:${RST}"
      echo -e "  选择 ${BOLD}1${RST} 安装 CLI"
      ;;
    *)
      echo -e "  ${R}✗ 无法自动打开下载页面${RST}"
      echo -e "  请手动访问: ${C}$REleases_URL${RST}"
      ;;
  esac
}

# ─── Agent 安装方式 ────────────────────────────────────────
show_agent_prompt() {
  echo ""
  echo -e "${BOLD}▶ Agent 安装（复制以下 prompt 给你的 AI agent）${RST}"
  echo ""
  local prompt="I want to set up SkillDo — an open-source skill manager for AI coding agents (https://github.com/yancongya/skilldo).\n\nPlease do the following:\n1. Star the repository (yancongya/skilldo) so I can find it later.\n2. Install the standalone CLI:\n   curl -fsSL https://raw.githubusercontent.com/yancongya/skilldo/main/scripts/install-cli.sh | bash\n3. Run \`skilldo status --json\` and tell me which of the 45 supported AI tools are installed on this machine.\n4. Run \`skilldo explore --json\` and recommend 3 featured skills worth installing for my stack."
  echo -e "${C}${prompt}${RST}"
  echo ""
  echo -e "  ${DIM}适用: Claude Code, Codex CLI, Cursor, WorkBuddy, 等支持 shell 命令的 agent${RST}"

  # 尝试复制到剪贴板
  if command -v pbcopy &>/dev/null; then
    echo -n -e "$prompt" | pbcopy
    echo -e "  ${G}✓ 已复制到剪贴板${RST}"
  elif command -v xclip &>/dev/null; then
    echo -n -e "$prompt" | xclip -selection clipboard
    echo -e "  ${G}✓ 已复制到剪贴板${RST}"
  elif command -v clip.exe &>/dev/null; then
    echo -n -e "$prompt" | clip.exe
    echo -e "  ${G}✓ 已复制到剪贴板${RST}"
  fi
}

# ─── 主菜单 ────────────────────────────────────────────────
clear 2>/dev/null || true
echo ""
echo -e "${BOLD}╔══════════════════════════════════════╗${RST}"
echo -e "${BOLD}║       ${C}SkillDo${RST}${BOLD} 安装向导              ║${RST}"
echo -e "${BOLD}║  Install once, sync everywhere.     ║${RST}"
echo -e "${BOLD}╚══════════════════════════════════════╝${RST}"
echo ""
echo -e "  ${DIM}平台: ${PLATFORM}${RST}"
check_installed

echo ""
echo -e "  ${BOLD}选择安装方式:${RST}"
echo ""
echo -e "    ${BOLD}1${RST}) ${G}CLI${RST}        独立二进制，一行命令安装，终端使用"
echo -e "    ${BOLD}2${RST}) ${B}Desktop${RST}    桌面客户端，可视化管理，支持自动更新"
echo -e "    ${BOLD}3${RST}) ${Y}Agent${RST}      复制 prompt 给 AI agent，让它帮你装"
echo -e "    ${BOLD}q${RST}) ${DIM}退出${RST}"
echo ""

read -r -p "  请选择 [1/2/3/q]: " choice

case "$choice" in
  1) install_cli ;;
  2) install_desktop ;;
  3) show_agent_prompt ;;
  q|Q|"") echo -e "  ${DIM}已退出${RST}" ;;
  *) echo -e "  ${R}无效选择: $choice${RST}" ;;
esac

echo ""
