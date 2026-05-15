#!/usr/bin/env bash
set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"

CMD="${1:-installer}"

case "$CMD" in
  installer)
    if ! command -v cargo-packager >/dev/null 2>&1; then
      echo "Installing cargo-packager..."
      cargo install cargo-packager --locked
    fi
    if [ ! -f assets/icon.png ]; then
      echo "Generating assets/icon.png..."
      cargo run --example gen_icon
    fi
    # Keep macOS Info.plist version in sync with Cargo.toml
    if [[ "$OSTYPE" == "darwin"* ]] && [ -f macos/Info.plist ]; then
      VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
      /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" macos/Info.plist
      /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" macos/Info.plist
      echo "Info.plist version set to $VERSION"
    fi
    echo "Building release binary..."
    cargo build --release
    echo "Packaging macOS .dmg..."
    cargo packager --release
    echo
    echo "========================================"
    echo " Installer built. Files in dist/:"
    echo "========================================"
    ls dist/ 2>/dev/null || true
    ;;
  dev)
    cargo build
    echo
    echo "Debug build OK. Running..."
    cargo run
    ;;
  release)
    cargo build --release
    echo "Release binary: target/release/tts-read"
    ;;
  clean)
    cargo clean
    ;;
  *)
    echo "Usage: ./build.sh [installer|dev|release|clean]"
    echo "  installer (default) build release + .dmg in dist/"
    echo "  dev                 debug build + run (for local testing)"
    echo "  release             optimized release binary, no installer"
    echo "  clean               cargo clean"
    exit 1
    ;;
esac
