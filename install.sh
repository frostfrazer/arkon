#!/usr/bin/env sh
# ARKON installer — https://arkon.sh
# Usage: curl -fsSL https://arkon.sh/install.sh | sh
set -e

REPO="https://github.com/arkon-sh/arkon"
BIN_NAME="arkon"
INSTALL_DIR="${ARKON_INSTALL_DIR:-$HOME/.local/bin}"

# ─── Detect platform ──────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64)  TARGET="x86_64-unknown-linux-musl" ;;
      aarch64) TARGET="aarch64-unknown-linux-musl" ;;
      *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac ;;
  Darwin)
    case "$ARCH" in
      x86_64)  TARGET="x86_64-apple-darwin" ;;
      arm64)   TARGET="aarch64-apple-darwin" ;;
      *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac ;;
  *)
    echo "Unsupported OS: $OS"
    echo "On Windows, use: cargo install --git $REPO arkon-cli"
    exit 1 ;;
esac

# ─── Detect version ───────────────────────────────────────────────────────────
if [ -n "$ARKON_VERSION" ]; then
  VERSION="$ARKON_VERSION"
else
  VERSION="$(curl -fsSL "https://api.github.com/repos/arkon-sh/arkon/releases/latest" \
    | grep '"tag_name"' | sed 's/.*"v\([^"]*\)".*/\1/')"
fi

if [ -z "$VERSION" ]; then
  echo "Could not determine latest ARKON version."
  echo "Set ARKON_VERSION=x.y.z to install a specific version."
  echo "Alternatively, build from source: cargo install --git $REPO arkon-cli"
  exit 1
fi

echo "Installing ARKON v$VERSION for $TARGET"

# ─── Download ─────────────────────────────────────────────────────────────────
TARBALL="arkon-v${VERSION}-${TARGET}.tar.gz"
DOWNLOAD_URL="$REPO/releases/download/v${VERSION}/${TARBALL}"
TMP_DIR="$(mktemp -d)"

echo "Downloading $DOWNLOAD_URL"
curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$TARBALL"

# ─── Install ──────────────────────────────────────────────────────────────────
mkdir -p "$INSTALL_DIR"
tar -xzf "$TMP_DIR/$TARBALL" -C "$TMP_DIR"
mv "$TMP_DIR/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"
rm -rf "$TMP_DIR"

echo "ARKON installed to $INSTALL_DIR/$BIN_NAME"

# ─── PATH check ───────────────────────────────────────────────────────────────
if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
  echo ""
  echo "Add $INSTALL_DIR to your PATH:"
  echo "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.bashrc  # bash"
  echo "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.zshrc   # zsh"
fi

echo ""
echo "  arkon --version"
echo "  arkon init"
echo ""
echo "Docs: https://arkon.sh/docs"
