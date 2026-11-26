#!/usr/bin/env bash
# LanPipe Linux 构建脚本。可在原生 Linux、WSL、容器或 CI 中运行。
#
# 用法:
#   tools/build-linux.sh deps       检查构建依赖
#   tools/build-linux.sh cli        构建 CLI 静态二进制 -> dist-linux/
#   tools/build-linux.sh desktop    构建桌面程序（不含安装包）-> dist-linux/
#   tools/build-linux.sh bundle     构建 deb + AppImage -> dist-linux/
#   tools/build-linux.sh all        cli + bundle
#   tools/build-linux.sh clean      清理 dist-linux/
#
# 环境变量:
#   SKIP_FRONTEND=1   跳过 npm 构建（dist/ 已存在时可用）
#   BUNDLES=deb       指定安装包类型（默认 deb,appimage）
#   CLI_TARGET=x86_64-unknown-linux-musl  生成静态 CLI（需要 rustup target add）

set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="$ROOT/dist-linux"
BUNDLES="${BUNDLES:-deb,appimage}"
SKIP_FRONTEND="${SKIP_FRONTEND:-0}"
CLI_TARGET="${CLI_TARGET:-}"

APT_DEPS=(
  build-essential curl wget file pkg-config
  libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev
  libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev patchelf
)

say() { printf '\n== %s\n' "$*"; }
ok() { printf '  [OK] %s\n' "$*"; }
bad() { printf '  [MISSING] %s\n' "$*"; }

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    bad "命令 $1"
    return 1
  fi
  ok "$1"
}

pkg_ok() {
  if pkg-config --exists "$1" 2>/dev/null; then
    ok "$1 $(pkg-config --modversion "$1" 2>/dev/null)"
    return 0
  fi
  bad "$1"
  return 1
}

print_apt_hint() {
  printf '\n  安装依赖（Debian/Ubuntu）:\n'
  printf '    sudo apt-get update\n'
  printf '    sudo apt-get install -y %s\n' "${APT_DEPS[*]}"
  printf '  Fedora:\n'
  printf '    sudo dnf install -y gcc gcc-c++ make pkgconf-pkg-config openssl-devel gtk3-devel webkit2gtk4.1-devel javascriptcoregtk4.1-devel libsoup3-devel libappindicator-gtk3-devel librsvg2-devel libxdo-devel patchelf\n'
  printf '  Arch:\n'
  printf '    sudo pacman -S --needed base-devel pkgconf openssl gtk3 webkit2gtk-4.1 libsoup3 libayatana-appindicator librsvg xdotool patchelf\n'
}

cmd_deps() {
  local fails=0
  say "检查工具链"
  need_cmd cc || fails=$((fails + 1))
  need_cmd pkg-config || fails=$((fails + 1))
  need_cmd cargo || fails=$((fails + 1))
  need_cmd rustc || fails=$((fails + 1))

  say "检查桌面端系统库"
  for mod in gtk+-3.0 webkit2gtk-4.1 javascriptcoregtk-4.1 libsoup-3.0 ayatana-appindicator3-0.1 librsvg-2.0; do
    pkg_ok "$mod" || fails=$((fails + 1))
  done

  say "检查打包工具"
  need_cmd patchelf || fails=$((fails + 1))
  need_cmd file || fails=$((fails + 1))
  need_cmd dpkg-deb || fails=$((fails + 1))

  say "检查前端工具"
  if [ "$SKIP_FRONTEND" = "1" ]; then
    ok "SKIP_FRONTEND=1，跳过 node/npm"
  else
    need_cmd node || fails=$((fails + 1))
    need_cmd npm || fails=$((fails + 1))
  fi

  if [ "$fails" -ne 0 ]; then
    printf '\n  缺少 %s 项依赖。\n' "$fails"
    print_apt_hint
    return 1
  fi
  printf '\n  依赖齐全。\n'
}

build_frontend() {
  if [ "$SKIP_FRONTEND" = "1" ]; then
    say "跳过前端构建（SKIP_FRONTEND=1）"
    if [ ! -f "$ROOT/dist/index.html" ]; then
      printf '  [ERROR] dist/index.html 不存在，无法跳过前端构建。\n' >&2
      exit 1
    fi
    return 0
  fi
  say "构建前端"
  if [ ! -d "$ROOT/node_modules" ]; then
    npm ci
  fi
  npm run build
}

build_cli() {
  say "构建 CLI"
  if [ -n "$CLI_TARGET" ]; then
    cargo build -p lanpipe-cli --release --target "$CLI_TARGET"
    CLI_BIN="$ROOT/target/$CLI_TARGET/release/lanpipe-cli"
  else
    cargo build -p lanpipe-cli --release
    CLI_BIN="$ROOT/target/release/lanpipe-cli"
  fi
  mkdir -p "$OUT"
  install -m 0755 "$CLI_BIN" "$OUT/lanpipe-cli"
  file "$OUT/lanpipe-cli"
  ls -l "$OUT/lanpipe-cli"
}

build_desktop() {
  build_frontend
  say "构建桌面程序"
  cargo build -p lanpipe --release --features custom-protocol
  mkdir -p "$OUT"
  install -m 0755 "$ROOT/target/release/lanpipe" "$OUT/lanpipe"
  file "$OUT/lanpipe"
  ls -l "$OUT/lanpipe"
}

tauri_cli() {
  if [ -x "$ROOT/node_modules/.bin/tauri" ]; then
    "$ROOT/node_modules/.bin/tauri" "$@"
  else
    npx --no-install tauri "$@"
  fi
}

build_bundle() {
  cmd_deps
  build_frontend
  say "构建安装包（$BUNDLES）"
  tauri_cli build --bundles "$BUNDLES"
  mkdir -p "$OUT"
  if [ -d "$ROOT/target/release/bundle/deb" ]; then
    cp -f "$ROOT"/target/release/bundle/deb/*.deb "$OUT"/ 2>/dev/null || true
  fi
  if [ -d "$ROOT/target/release/bundle/appimage" ]; then
    cp -f "$ROOT"/target/release/bundle/appimage/*.AppImage "$OUT"/ 2>/dev/null || true
  fi
  say "产物"
  ls -l "$OUT"
}

cmd_clean() {
  say "清理 dist-linux"
  rm -rf "$OUT"
  ok "已删除 $OUT"
}

case "${1:-help}" in
  deps) cmd_deps ;;
  cli) build_cli ;;
  desktop) build_desktop ;;
  bundle) build_bundle ;;
  all)
    build_cli
    build_bundle
    ;;
  clean) cmd_clean ;;
  help | -h | --help)
    sed -n '2,22p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    ;;
  *)
    printf '未知命令: %s\n' "${1:-}" >&2
    printf '可用命令: deps | cli | desktop | bundle | all | clean\n' >&2
    exit 2
    ;;
esac
