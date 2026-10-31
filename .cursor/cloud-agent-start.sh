#!/usr/bin/env bash
set -euo pipefail

if [[ ! -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]]; then
  echo "Nix is not installed; run cloud-agent-install.sh first." >&2
  exit 1
fi

# shellcheck source=/dev/null
. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh

if ! nix config check >/dev/null 2>&1; then
  if ! pgrep -x nix-daemon >/dev/null 2>&1; then
    sudo /nix/var/nix/profiles/default/bin/nix-daemon &
  fi
  for _ in $(seq 1 60); do
    if nix config check >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
fi

if ! nix config check >/dev/null 2>&1; then
  echo "nix-daemon did not become ready; see nix-daemon logs." >&2
  exit 1
fi
