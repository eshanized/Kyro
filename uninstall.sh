#!/usr/bin/env bash
#
# Kyro Uninstaller Script
# https://github.com/eshanized/Kyro
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/eshanized/Kyro/master/uninstall.sh | bash
#   ./uninstall.sh --purge
#
set -euo pipefail

REPO="eshanized/Kyro"
CUSTOM_PREFIX=""
PURGE=0
CONFIRM=1

if [ -t 1 ]; then
  BOLD="\033[1m"
  GREEN="\033[1;32m"
  BLUE="\033[1;34m"
  YELLOW="\033[1;33m"
  RED="\033[1;31m"
  RESET="\033[0m"
else
  BOLD=""
  GREEN=""
  BLUE=""
  YELLOW=""
  RED=""
  RESET=""
fi

info() { echo -e "${BLUE}==>${RESET} ${BOLD}$*${RESET}"; }
success() { echo -e "${GREEN}==>${RESET} ${BOLD}$*${RESET}"; }
warn() { echo -e "${YELLOW}WARNING:${RESET} $*"; }
error() { echo -e "${RED}ERROR:${RESET} $*" >&2; exit 1; }

usage() {
  cat << EOF
Kyro Uninstaller

Usage:
  curl -fsSL https://raw.githubusercontent.com/eshanized/Kyro/master/uninstall.sh | bash [options]
  ./uninstall.sh [options]

Options:
  -p, --prefix <DIR>     Explicit directory prefix to uninstall from
  --purge                Also delete local user data, settings, and SQLite storage
  -y, --yes              Skip confirmation prompt
  -h, --help             Show this help message
EOF
  exit 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -p|--prefix)
      CUSTOM_PREFIX="$2"
      shift 2
      ;;
    --purge)
      PURGE=1
      shift
      ;;
    -y|--yes)
      CONFIRM=0
      shift
      ;;
    -h|--help)
      usage
      ;;
    *)
      warn "Unknown option '$1'"
      usage
      ;;
  esac
done

if [ -n "$CUSTOM_PREFIX" ]; then
  PREFIXES=("$CUSTOM_PREFIX")
else
  PREFIXES=("$HOME/.local" "/usr/local" "/usr")
fi

FOUND=0
for prefix in "${PREFIXES[@]}"; do
  if [ -f "$prefix/bin/kyro" ] || [ -f "$prefix/share/applications/kyro.desktop" ]; then
    FOUND=1
  fi
done

if [ "$FOUND" -eq 0 ]; then
  info "Kyro executable or desktop entry was not found in common prefixes (${PREFIXES[*]})."
fi

echo -e "${BOLD}Kyro Uninstallation${RESET}"
echo "Target paths to remove:"
for prefix in "${PREFIXES[@]}"; do
  [ -f "$prefix/bin/kyro" ] && echo "  - $prefix/bin/kyro"
  [ -f "$prefix/share/applications/kyro.desktop" ] && echo "  - $prefix/share/applications/kyro.desktop"
  [ -f "$prefix/share/applications/Kyro.desktop" ] && echo "  - $prefix/share/applications/Kyro.desktop"
  [ -f "$prefix/share/icons/hicolor/512x512/apps/kyro.png" ] && echo "  - $prefix/share/icons/hicolor/512x512/apps/kyro.png"
done

if [ "$PURGE" -eq 1 ]; then
  echo "Data directories to purge:"
  [ -d "$HOME/.local/share/com.kyro.agent" ] && echo "  - $HOME/.local/share/com.kyro.agent"
  [ -d "$HOME/.config/com.kyro.agent" ] && echo "  - $HOME/.config/com.kyro.agent"
  [ -d "$HOME/.local/share/kyro" ] && echo "  - $HOME/.local/share/kyro"
fi

if [ "$CONFIRM" -eq 1 ] && [ -t 0 ]; then
  echo ""
  read -r -p "Do you want to proceed with uninstallation? [y/N] " response
  case "$response" in
    [yY][eE][sS]|[yY]) ;;
    *) echo "Aborted."; exit 0 ;;
  esac
fi

for prefix in "${PREFIXES[@]}"; do
  if [ -w "$prefix/bin" ] || [ "$(id -u)" -eq 0 ]; then
    rm -f "$prefix/bin/kyro"
    rm -f "$prefix/share/applications/kyro.desktop"
    rm -f "$prefix/share/applications/Kyro.desktop"
    rm -f "$prefix/share/icons/hicolor/512x512/apps/kyro.png"
  else
    if [ -f "$prefix/bin/kyro" ]; then
      warn "Permission denied writing to $prefix. Try running with sudo: sudo ./uninstall.sh --prefix $prefix"
    fi
  fi

  if [ -d "$prefix/share/applications" ] && command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$prefix/share/applications" 2>/dev/null || true
  fi
done

if [ "$PURGE" -eq 1 ]; then
  info "Purging local Kyro databases and preferences..."
  rm -rf "$HOME/.local/share/com.kyro.agent"
  rm -rf "$HOME/.config/com.kyro.agent"
  rm -rf "$HOME/.local/share/kyro"
fi

echo ""
success "Kyro has been successfully uninstalled from your system."
echo ""
