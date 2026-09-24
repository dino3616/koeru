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
# OpenUtau は `.sha256` を公開していないが、GitHub の API が資産の digest を
# 返す。100 MB 超を落とさずに取れる（`DEC-SYN-010` の層B）。
#
# # 版の正本
#
# bun は `package.json` の `packageManager`、fslc は `flake.nix` の `fslcVersion`。
# ここでは版を決めず、読むだけ（`DEC-PLT-033`）。

set -euo pipefail

cd "$(dirname "$0")/../.."

readonly FLAKE=flake.nix
readonly PACKAGE_JSON=crates/koeru-app/ui/package.json
readonly OPENUTAU=crates/koeru-core/fixtures/phonemizer-parity/openutau.toml

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

# ## OpenUtau（`DEC-SYN-010` の層B）
#
# チャンネルごとに版と資産名を持ち、指紋だけをここで当てる。
# **版は Renovate が決める。** ここは読むだけ（`DEC-PLT-033` と同じ分担）。
#
# 落として測らない。 GitHub の API が upload 時の digest を返すので、
# 100 MB 超を引かずに済む。CI 側は引いたバイトをこの値と突き合わせる。
# `key = "value"` から value を取る。 **`sub(/.*"/)` は使わない**——
# 貪欲なので最後の引用符まで消え、空文字が返る。**踏んだ。**
readonly OU_FIELD='
  /^name *=/ { split($0, a, "\""); n = a[2] }
  n == c && $1 == k { split($0, a, "\""); print a[2]; exit }
'
openutau_field() { awk -v c="$1" -v k="$2" "$OU_FIELD" "$OPENUTAU"; }

for channel in stable alpha; do
  v=$(openutau_field "$channel" version)
  a=$(openutau_field "$channel" asset)
  [ -n "$v" ] && [ -n "$a" ] || { echo "$OPENUTAU から $channel を読めない" >&2; exit 1; }
  echo "OpenUtau $channel $v"
  hex=$(curl -fsSL \
      -H 'Accept: application/vnd.github+json' \
      -H 'User-Agent: koeru-hash-fix' \
      "https://api.github.com/repos/stakira/OpenUtau/releases/tags/$v" \
    | python3 -c "
import json, sys
want = sys.argv[1]
for asset in json.load(sys.stdin)['assets']:
    if asset['name'] == want:
        print((asset.get('digest') or '').removeprefix('sha256:'))
        break
" "$a")
  [ -n "$hex" ] || { echo "digest が無い: $v / $a" >&2; exit 1; }
  # そのチャンネルの節の中の sha256 だけを差し替える。
  awk -v c="$channel" -v h="$hex" '
    /^name *=/ { split($0, a, "\""); n = a[2] }
    n == c && $1 == "sha256" { print "sha256 = \"" h "\""; next }
    { print }
  ' "$OPENUTAU" > "$OPENUTAU.new"
  mv "$OPENUTAU.new" "$OPENUTAU"
  echo "  $a $hex"
done
