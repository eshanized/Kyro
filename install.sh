#!/usr/bin/env bash
#
# Kyro Installer Script
# https://github.com/eshanized/Kyro
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/eshanized/Kyro/master/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/eshanized/Kyro/master/install.sh | bash -s -- --prefix /usr/local
#
set -euo pipefail

REPO="eshanized/Kyro"
DEFAULT_VERSION=""
CUSTOM_PREFIX=""
DRY_RUN=0

# Formatting
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
Kyro Desktop AI Agent Installer

Usage:
  curl -fsSL https://raw.githubusercontent.com/eshanized/Kyro/master/install.sh | bash [options]
  ./install.sh [options]

Options:
  -p, --prefix <DIR>     Installation directory prefix (default: ~/.local, or /usr/local if root)
  -v, --version <VER>    Install specific version (e.g. v0.1.2 or 0.1.2, default: latest)
  --dry-run              Print intended actions without installing
  --uninstall            Run uninstaller
  -h, --help             Show this help message
EOF
  exit 0
}

# Parse flags
while [[ $# -gt 0 ]]; do
  case "$1" in
    -p|--prefix)
      CUSTOM_PREFIX="$2"
      shift 2
      ;;
    -v|--version)
      DEFAULT_VERSION="$2"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --uninstall)
      SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
      if [ -f "$SCRIPT_DIR/uninstall.sh" ]; then
        exec bash "$SCRIPT_DIR/uninstall.sh"
      else
        exec curl -fsSL "https://raw.githubusercontent.com/eshanized/Kyro/master/uninstall.sh" | bash
      fi
      exit 0
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

# Check OS
OS="$(uname -s)"
if [ "$OS" != "Linux" ]; then
  error "Kyro is currently only packaged for Linux. Detected OS: $OS"
fi

# Check architecture
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64)
    ARCH_SUFFIX="linux-x86_64"
    ;;
  *)
    error "Unsupported architecture: $ARCH. Kyro currently provides prebuilt binaries for x86_64 (amd64)."
    ;;
esac

# Check dependencies
for cmd in curl tar gzip sha256sum; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    error "Required command '$cmd' is not installed or not in PATH."
  fi
done

# Determine installation prefix
if [ -n "$CUSTOM_PREFIX" ]; then
  PREFIX="$CUSTOM_PREFIX"
elif [ "$(id -u)" -eq 0 ]; then
  PREFIX="/usr/local"
else
  PREFIX="$HOME/.local"
fi

BIN_DIR="$PREFIX/bin"
APP_DIR="$PREFIX/share/applications"
ICON_DIR="$PREFIX/share/icons/hicolor/512x512/apps"

info "Target installation directory: $BIN_DIR/kyro"

# Resolve version
if [ -z "$DEFAULT_VERSION" ]; then
  info "Resolving latest Kyro release from GitHub..."
  LATEST_JSON="$(curl -fsSL -H "Accept: application/vnd.github.v3+json" "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null || true)"
  if [ -n "$LATEST_JSON" ]; then
    VERSION="$(echo "$LATEST_JSON" | grep -m1 '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)"
  fi
  if [ -z "${VERSION:-}" ]; then
    VERSION="v0.1.2"
  fi
else
  VERSION="$DEFAULT_VERSION"
fi

# Normalize tag name
if [[ "$VERSION" != v* ]]; then
  TAG="v$VERSION"
else
  TAG="$VERSION"
fi
CLEAN_VER="${TAG#v}"

info "Selected version: $TAG ($CLEAN_VER)"

ASSET_NAME="kyro-${CLEAN_VER}-${ARCH_SUFFIX}.tar.gz"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$TAG/$ASSET_NAME"
CHECKSUM_URL="https://github.com/$REPO/releases/download/$TAG/SHA256SUMS.txt"

if [ "$DRY_RUN" -eq 1 ]; then
  echo "--- DRY RUN PLAN ---"
  echo "Asset: $DOWNLOAD_URL"
  echo "Install Prefix: $PREFIX"
  echo "Target Binary: $BIN_DIR/kyro"
  echo "Target Desktop: $APP_DIR/kyro.desktop"
  echo "Target Icon: $ICON_DIR/kyro.png"
  exit 0
fi

# Create scratch directory
TMP_DIR="$(mktemp -d -t kyro-install-XXXXXX)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

info "Downloading $ASSET_NAME..."
curl -fSL --progress-bar "$DOWNLOAD_URL" -o "$TMP_DIR/$ASSET_NAME" || error "Failed to download $DOWNLOAD_URL. Please verify release version."

info "Verifying SHA-256 checksum..."
if curl -fsSL "$CHECKSUM_URL" -o "$TMP_DIR/SHA256SUMS.txt" 2>/dev/null; then
  EXPECTED_SUM="$(grep "$ASSET_NAME" "$TMP_DIR/SHA256SUMS.txt" | awk '{print $1}' || true)"
  if [ -n "$EXPECTED_SUM" ]; then
    ACTUAL_SUM="$(sha256sum "$TMP_DIR/$ASSET_NAME" | awk '{print $1}')"
    if [ "$EXPECTED_SUM" != "$ACTUAL_SUM" ]; then
      error "Checksum verification failed!\nExpected: $EXPECTED_SUM\nActual:   $ACTUAL_SUM"
    fi
    success "Checksum verified: $ACTUAL_SUM"
  else
    warn "Artifact $ASSET_NAME not found in SHA256SUMS.txt, skipping checksum verification."
  fi
else
  warn "Could not fetch SHA256SUMS.txt, proceeding without checksum verification."
fi

info "Extracting archive..."
tar -xzf "$TMP_DIR/$ASSET_NAME" -C "$TMP_DIR"

EXTRACTED_DIR="$TMP_DIR/kyro-${CLEAN_VER}-${ARCH_SUFFIX}"
if [ ! -d "$EXTRACTED_DIR" ]; then
  EXTRACTED_DIR="$TMP_DIR"
fi

if [ ! -f "$EXTRACTED_DIR/kyro" ]; then
  error "Extracted archive does not contain 'kyro' executable."
fi

# Create target directories
mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"

info "Installing executable to $BIN_DIR/kyro..."
cp "$EXTRACTED_DIR/kyro" "$BIN_DIR/kyro"
chmod 755 "$BIN_DIR/kyro"

# Install icon if present
if [ -f "$EXTRACTED_DIR/kyro.png" ]; then
  info "Installing desktop icon..."
  cp "$EXTRACTED_DIR/kyro.png" "$ICON_DIR/kyro.png"
fi

# Install desktop entry if present
if [ -f "$EXTRACTED_DIR/kyro.desktop" ]; then
  info "Installing desktop entry to $APP_DIR/kyro.desktop..."
  sed -e "s|^Exec=.*|Exec=$BIN_DIR/kyro|g" \
      -e "s|^Icon=.*|Icon=$ICON_DIR/kyro.png|g" \
      "$EXTRACTED_DIR/kyro.desktop" > "$APP_DIR/kyro.desktop"
  chmod 644 "$APP_DIR/kyro.desktop"
fi

# Refresh desktop caches if tools are available
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APP_DIR" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
fi

echo ""
success "Kyro $TAG installed successfully!"
echo ""

# Check PATH
PATH_OK=0
case ":$PATH:" in
  *":$BIN_DIR:"*) PATH_OK=1 ;;
esac

if [ "$PATH_OK" -eq 0 ]; then
  warn "$BIN_DIR is not currently in your PATH."
  echo ""
  echo "Add it to your current shell configuration by running:"
  echo ""
  case "${SHELL##*/}" in
    zsh)
      echo "  echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.zshrc && source ~/.zshrc"
      ;;
    fish)
      echo "  fish_add_path $BIN_DIR"
      ;;
    *)
      echo "  echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
      ;;
  esac
  echo ""
fi

echo -e "You can now start Kyro by running: ${BOLD}kyro${RESET}"
echo -e "To uninstall anytime, run:        ${BOLD}curl -fsSL https://raw.githubusercontent.com/$REPO/master/uninstall.sh | bash${RESET}"
echo ""
