#!/usr/bin/env bash
# Run a command in `nix develop` with LD_LIBRARY_PATH set so `cargo run` / Tauri
# binaries find WebKitGTK and libstdc++ (NIX_LDFLAGS only covers link-time rpath).
set -euo pipefail

cd "$(dirname "$0")/.."
bash .cursor/cloud-agent-start.sh

exec nix develop --command bash -c '
  set -euo pipefail
  libs=$(for f in $NIX_LDFLAGS; do case $f in -L*) echo ${f#-L};; esac; done | sort -u)
  stdcxx_lib=$(ls /nix/store/*gcc-*-lib/lib/libstdc++.so.6 2>/dev/null | head -1 || true)
  if [[ -n "$stdcxx_lib" ]]; then
    stdcxx=$(dirname "$stdcxx_lib")
  else
    stdcxx=$(dirname "$(readlink -f "$(which g++)")")/../lib
  fi
  export LD_LIBRARY_PATH="${stdcxx}:$(echo "$libs" | paste -sd:)"
  exec "$@"
' bash "$@"
