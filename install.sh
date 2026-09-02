#!/usr/bin/env bash
# Dashy contributor bootstrap for macOS and Linux.
#
#   ./install.sh --check-only   inspect the machine without changing it
#   ./install.sh                install what is missing
#
# The script never removes or downgrades a tool, never signs in to a provider,
# and never asks for a token. Provider CLI logins stay in your own terminal.
set -euo pipefail

CHECK_ONLY=0
for argument in "$@"; do
  case "$argument" in
    --check-only) CHECK_ONLY=1 ;;
    -h|--help)
      sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "Unknown option: $argument" >&2; exit 2 ;;
  esac
done

REQUIRED_COMMANDS=(git gh node npm rustc cargo cargo-tauri claude codex)
MIN_NODE_MAJOR=20
os="$(uname -s)"

say() { printf '%s\n' "$*"; }
warn() { printf '%s\n' "$*" >&2; }
have() { command -v "$1" >/dev/null 2>&1; }

node_is_recent() {
  have node || return 1
  local major
  major="$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)"
  [[ "$major" -ge "$MIN_NODE_MAJOR" ]]
}

pkg_config_has() {
  have pkg-config && pkg-config --exists "$1"
}

MISSING_COMMANDS=()

report_commands() {
  MISSING_COMMANDS=()
  for name in "${REQUIRED_COMMANDS[@]}"; do
    if have "$name"; then
      say "  found    $name"
    else
      say "  missing  $name"
      MISSING_COMMANDS+=("$name")
    fi
  done
  if have node && ! node_is_recent; then
    say "  outdated node ($(node --version)); Dashy needs Node.js $MIN_NODE_MAJOR or newer"
  fi
}

report_platform_prerequisites() {
  case "$os" in
    Darwin)
      if xcode-select -p >/dev/null 2>&1; then
        say "  found    Xcode Command Line Tools"
      else
        say "  missing  Xcode Command Line Tools"
      fi
      if have brew; then
        say "  found    Homebrew"
      else
        say "  missing  Homebrew (https://brew.sh)"
      fi
      ;;
    Linux)
      for module in webkit2gtk-4.1 gtk+-3.0 ayatana-appindicator3-0.1 librsvg-2.0; do
        if pkg_config_has "$module"; then
          say "  found    $module"
        else
          say "  missing  $module (development package)"
        fi
      done
      ;;
    *)
      warn "Unsupported operating system: $os. Use install.ps1 on Windows."
      exit 1
      ;;
  esac
}

install_macos() {
  if ! xcode-select -p >/dev/null 2>&1; then
    say "Requesting the Xcode Command Line Tools installer..."
    xcode-select --install || true
    warn "Finish the Command Line Tools installation, then rerun this script."
    exit 1
  fi
  if ! have brew; then
    warn "Homebrew is required to install the remaining tools: https://brew.sh"
    exit 1
  fi
  have git || brew install git
  have gh || brew install gh
  node_is_recent || brew install node
  if ! have rustup; then
    brew install rustup
  fi
  if ! have cargo; then
    rustup default stable
  fi
  have claude || brew install --cask claude-code
  have codex || brew install --cask codex
}

install_linux() {
  local packages=()
  pkg_config_has webkit2gtk-4.1 || packages+=(libwebkit2gtk-4.1-dev)
  pkg_config_has ayatana-appindicator3-0.1 || packages+=(libayatana-appindicator3-dev)
  pkg_config_has librsvg-2.0 || packages+=(librsvg2-dev)
  have pkg-config || packages+=(pkg-config)
  have git || packages+=(git)
  have gh || packages+=(gh)
  packages+=(build-essential curl libssl-dev libxdo-dev patchelf file)
  if have apt-get; then
    say "Installing system packages with apt-get (sudo may prompt): ${packages[*]}"
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends "${packages[@]}"
    if ! have rustup && ! have cargo; then
      sudo apt-get install -y rustup || warn "rustup is not packaged for this release; install it from https://rustup.rs"
    fi
  else
    warn "This script installs system packages with apt-get only. On other distributions install the equivalents of: ${packages[*]}"
    warn "See https://v2.tauri.app/start/prerequisites/ for the exact package names."
  fi
  if have rustup && ! have cargo; then
    rustup default stable
  fi
  if ! node_is_recent; then
    warn "Node.js $MIN_NODE_MAJOR or newer is required. Install it with your distribution's current package, nvm, or fnm, then rerun this script."
  fi
  have claude || warn "Install Claude Code from https://code.claude.com/docs/en/setup"
  have codex || warn "Install the Codex CLI from https://learn.chatgpt.com/docs/codex/cli"
}

install_tauri_cli() {
  if have cargo-tauri; then
    return
  fi
  if ! have cargo; then
    warn "cargo is not available; open a new shell after installing Rust and rerun this script."
    return
  fi
  say "Installing Tauri CLI 2 with cargo..."
  cargo install tauri-cli --version '^2' --locked
}

install_frontend_dependencies() {
  if ! node_is_recent; then
    warn "Skipping npm ci until Node.js $MIN_NODE_MAJOR or newer is available."
    return
  fi
  say "Installing locked frontend dependencies..."
  (cd "$(dirname "$0")/frontend" && npm ci)
}

say "Dashy contributor prerequisites on $os"
say "Commands:"
report_commands
say "Platform:"
report_platform_prerequisites

if [[ "$CHECK_ONLY" -eq 1 ]]; then
  say "Check-only mode: nothing was installed or changed."
  exit 0
fi

case "$os" in
  Darwin) install_macos ;;
  Linux) install_linux ;;
esac
# Homebrew and rustup put their bin directories on PATH for new shells only.
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
install_tauri_cli
install_frontend_dependencies

say "Done. Commands after this run:"
report_commands
if [[ "${#MISSING_COMMANDS[@]}" -ne 0 ]]; then
  say "Open a new shell and run ./install.sh --check-only again; install anything still listed with its official installer."
fi
say "Sign in to only the provider CLIs you need (gh auth login, claude auth login, codex login, grok login, cursor-agent login)."
