#!/usr/bin/env bash
# `flake.nix` の SHA-256 を、上流が公開している digest から作り直す。
#
# # なぜ上流の digest を読むのか
#
# ビルドの失敗から直す方式（`determinate-nixd fix hashes`）では足りない。
# hash は 3 system × 2 ツールで 6 個あり、ubuntu のビルドは x86_64-linux の分しか
# 取得しないので、macOS と arm64-linux の分が古いまま通ってしまう。
#
# bun は `SHASUMS256.txt`、fsl は資産ごとの `.sha256` を公開している。
# そこを読めば、バイナリを落とさずに 6 個すべてを確定的に作れる。
#
# # 版の正本
#
# bun は `package.json` の `packageManager`、fslc は `flake.nix` の `fslcVersion`。
# ここでは版を決めず、読むだけ（`DEC-PLT-033`）。

set -euo pipefail

cd "$(dirname "$0")/.."

readonly FLAKE=flake.nix
readonly PACKAGE_JSON=crates/koeru-app/ui/package.json

sri() { nix hash convert --hash-algo sha256 --to sri "$1"; }

# 資産名の次の行にある hash を置き換える。 `-i.bak` は BSD sed と GNU sed の
# どちらでも通る形。 macOS でも ubuntu でも同じ結果にしたい。
put() {
  sed -i.bak "/name = \"$1\";/{n;s|hash = \"[^\"]*\"|hash = \"$2\"|;}" "$FLAKE"
  rm -f "$FLAKE.bak"
}

bun_version=$(sed -n 's/.*"packageManager": *"bun@\([^"]*\)".*/\1/p' "$PACKAGE_JSON")
[ -n "$bun_version" ] || { echo "packageManager から bun の版が読めない" >&2; exit 1; }
echo "bun $bun_version"
sums=$(curl -fsSL "https://github.com/oven-sh/bun/releases/download/bun-v$bun_version/SHASUMS256.txt")
for asset in bun-darwin-aarch64 bun-linux-aarch64 bun-linux-x64; do
  hex=$(printf '%s\n' "$sums" | awk -v want="$asset.zip" '$2 == want { print $1 }')
  [ -n "$hex" ] || { echo "digest が無い: $asset.zip" >&2; exit 1; }
  h=$(sri "$hex")
  put "$asset" "$h"
  echo "  $asset $h"
done

fslc_version=$(sed -n 's/.*fslcVersion = "\([^"]*\)".*/\1/p' "$FLAKE")
[ -n "$fslc_version" ] || { echo "flake.nix から fslc の版が読めない" >&2; exit 1; }
echo "fslc $fslc_version"
for asset in fslc-macos-arm64 fslc-linux-x64 fslc-linux-arm64; do
  hex=$(curl -fsSL "https://github.com/ymm-oss/fsl/releases/download/$fslc_version/$asset.sha256" | awk '{ print $1 }')
  [ -n "$hex" ] || { echo "digest が無い: $asset" >&2; exit 1; }
  h=$(sri "$hex")
  put "$asset" "$h"
  echo "  $asset $h"
done
