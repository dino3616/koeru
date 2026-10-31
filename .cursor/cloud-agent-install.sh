#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

bash .cursor/cloud-agent-start.sh

if ! command -v nix >/dev/null; then
  curl -fsSL https://install.determinate.systems/nix | sh -s -- install --determinate --no-confirm
  # shellcheck source=/dev/null
  . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
  bash .cursor/cloud-agent-start.sh
fi

# Headless Tauri (Xvfb from nixpkgs, not host apt).
if ! command -v Xvfb >/dev/null || [[ "$(command -v Xvfb)" == /usr/bin/Xvfb ]]; then
  nix profile install nixpkgs#xorgserver
fi

nix develop --command git lfs install
git submodule update --init --recursive
nix develop --command git lfs pull

nix develop --command bash -c 'cd crates/koeru-app/ui && bun install && bun run build'
