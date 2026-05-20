#!/usr/bin/env sh
# ARKON installer
# Usage: curl -fsSL https://raw.githubusercontent.com/frostfrazer/arkon/main/install.sh | sh
set -e

REPO_URL="https://github.com/frostfrazer/arkon"
BIN_NAME="arkon"
INSTALL_DIR="${ARKON_INSTALL_DIR:-$HOME/.local/bin}"

echo ""
echo "  ┌─────────────────────────────────────┐"
echo "  │   ARKON — automated deploy toolkit  │"
echo "  └─────────────────────────────────────┘"
echo ""

# ── Detect OS ────────────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux|Darwin) ;;
  *)
    echo "Windows detected."
    echo "Install via cargo:"
    echo "  cargo install --git $REPO_URL arkon-cli"
    exit 0 ;;
esac

# ── Check for cargo ───────────────────────────────────────────────────────────
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust not found. Installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
  . "$HOME/.cargo/env"
fi

# ── Install arkon ─────────────────────────────────────────────────────────────
echo "Installing ARKON from source (this takes 2-3 minutes)..."
echo ""

cargo install --git "$REPO_URL" arkon-cli --locked 2>&1

# ── Find the binary ───────────────────────────────────────────────────────────
CARGO_BIN="$HOME/.cargo/bin/$BIN_NAME"

if [ -f "$CARGO_BIN" ]; then
  mkdir -p "$INSTALL_DIR"
  cp "$CARGO_BIN" "$INSTALL_DIR/$BIN_NAME"
  echo ""
  echo "  ✓  ARKON installed to $INSTALL_DIR/$BIN_NAME"
else
  echo ""
  echo "  ✓  ARKON installed to $HOME/.cargo/bin/$BIN_NAME"
  INSTALL_DIR="$HOME/.cargo/bin"
fi

# ── PATH reminder ─────────────────────────────────────────────────────────────
if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
  echo ""
  echo "  Add to your PATH:"
  echo "    echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.bashrc"
  echo "    source ~/.bashrc"
fi

# ── Set default relay ─────────────────────────────────────────────────────────
ARKON_CONFIG_DIR="$HOME/.arkon"
mkdir -p "$ARKON_CONFIG_DIR"
if [ ! -f "$ARKON_CONFIG_DIR/config.toml" ]; then
  cat > "$ARKON_CONFIG_DIR/config.toml" <<EOF
# ARKON global config
relay_url = "https://arkon-relay.onrender.com"
EOF
  echo "  ✓  Default relay set: https://arkon-relay.onrender.com"
fi

# ── Done ──────────────────────────────────────────────────────────────────────
echo ""
echo "  Get started:"
echo ""
echo "    arkon --version"
echo "    cd your-project"
echo "    arkon init"
echo "    arkon ship"
echo ""
echo "  Docs: https://github.com/frostfrazer/arkon"
echo ""
