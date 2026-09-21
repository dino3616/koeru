---
name: setup-koeru
description: KOERU の開発環境を用意する手順。Nix を第一級の前提とし、devShell が開発ツールを供給する。Nix の導入、direnv、git-lfs と submodule の順序、Nix で覆えないもの（Windows・Xcode CLT・Intel Mac）、cloud とエージェントでの使い方、WSL の差、nixpkgs 追従の制約を定める。環境を用意するとき、clone が途中で死んだとき、ツールの版が CI と食い違うとき、貢献者の環境を案内するときに使う。
---

# KOERU — 環境を用意する

**開発ツールは `flake.nix` が供給する**（`DEC-PLT-033`）。 rustup も Homebrew も
apt も要らない。何をどの順に走らせるかは `verify-koeru` が持つ。

Nix が供給するのは「OS 固有の SDK 以外の開発ツール」。 供給しないものは
[覆えないもの](#覆えないもの)に挙げてある。

## 1. Nix を入れる

```bash
curl -fsSL https://install.determinate.systems/nix | sh -s -- install --determinate
```

**GID 350 か UID 351 が埋まっていると落ちる。** 企業の端末管理ソフトがこの帯を
使っていることがある（`_avectodaemon` が GID 350、`_defendpoint` が UID 351 を
持っていた）。`create_group` が `GID already exists` で失敗する。**踏んだ。**

空いている帯を探して、両方ずらす。**最初の UID は base + 1** なので、
グループと同じ数を渡せば1つ上から並ぶ。

```bash
# 何が埋まっているかを見る
dscl . -list /Groups PrimaryGroupID | awk '$2>=340 && $2<=420'
dscl . -list /Users  UniqueID       | awk '$2>=340 && $2<=420'

curl -fsSL https://install.determinate.systems/nix | sh -s -- install --determinate \
  --nix-build-group-id 360 --nix-build-user-id-base 360
```

失敗しても綺麗に戻る。 インストーラが revert を提案するので受ければよい。
掃除してから入れ直す必要は無い。

ずらした場合、後で nix-darwin を使うなら `ids.gids.nixbld` を同じ値にすること。

## 2. clone する

**トップレベルだけ先に clone する。** `git-lfs` は devShell が供給するので、
submodule を取るのはシェルに入ってから。これで Homebrew の前提が消える。

```bash
git clone https://github.com/dino3616/koeru
cd koeru
nix develop                              # ここで git-lfs が PATH に乗る
git lfs install
git submodule update --init --recursive
```

**順序を守る。** `--recurse-submodules` を付けて clone すると、`git-lfs` が
無い環境では `git-lfs filter-process: command not found` で途中で死ぬ
（`DEC-ALN-012`）。**一度やった。**

C / C++ とモデルの正本は git のまま（`DEC-PLT-016`、`DEC-ALN-012`）。
flake は `inputs.self.submodules` を立てていない——立てると Kaldi（約123MB）と
音響モデルまで store へ複製される。

モデルが無くてもアプリは動く。 自動原音設定が音響モデルを使わない退避経路に落ちるだけ。

## 3. direnv を入れる（任意だが推奨）

素の `nix develop` は毎回 flake を再評価するので数秒かかり、GC root を張らないので
`nix-collect-garbage` で devShell が消える。[nix-direnv](https://github.com/nix-community/nix-direnv)
はキャッシュして root を張る。

```bash
nix profile install nixpkgs#direnv nixpkgs#nix-direnv
# シェルの設定に eval "$(direnv hook zsh)" を足す
direnv allow
```

`.envrc` は commit してある。`.direnv/` は `.gitignore` に入っている。

**worktree ごとに `direnv allow` が1回要る。** このリポジトリは
`.claude/worktrees/` 以下で作業するので、worktree を作るたびに実行する。

`flake.nix` を触った直後に再構築が走って作業が止まるなら、nix-direnv の
manual reload モードにする。

## 4. 編集機

`nixd` が devShell に入っている。PATH から拾うので、機械ごとの設定は要らない。
`nil` ではなく `nixd` なのは、nixpkgs を実際に評価して補完するから——
`nil` は静的解析だけで、パッケージ名が出てこない。

整形は `nix fmt`（RFC 166）。CI が `nixfmt --check` で通過を強制する。

## エージェントと cloud

**`nix develop --command` で明示的に包む。** direnv に頼らない——シェルのフックに
依存しない形のほうが決定的で、`flake.lock` が同じなら機械が変わっても同じ版になる。

```bash
nix develop --command cargo test --workspace --all-features
```

初回は重い。 Kaldi と LFS のモデルに加えて Nix の closure を引く。
使い捨ての機械で回すなら、`cache.nixos.org` に当たることを確かめてから並べる。

## WSL

**Ubuntu-WSL に Nix パッケージマネージャを入れる形を推す。** ホストが FHS なので、
ダウンロード版のバイナリも動く。

**NixOS-WSL だと FHS が無い。** Playwright のダウンロード版ブラウザは動的リンカに
届かず起動しない。flake は `playwright-driver.browsers` を `PLAYWRIGHT_BROWSERS_PATH`
に渡しているので、どちらでも動くようにしてある——**ブラウザを自分で入れ直さないこと。**

## 覆えないもの

| | なぜ |
|---|---|
| **Windows** | Nix がネイティブに支えない。公式経路の WSL2 は Linux 環境なので、Windows ネイティブの検証にならない。`TR-PLT-01` は Windows を第一級の**対象**としているので、ここは非対称が残る。検証経路は CI の windows ジョブだけ |
| **Xcode Command Line Tools** | Tauri 自身が macOS 開発で要求する。`koeru-align` が Accelerate にリンクするのも SDK 経由。`xcode-select --install` は別に要る |
| **Intel Mac（x86_64-darwin）** | nixpkgs 26.11 が対応を打ち切っており、評価そのものが失敗する。`TR-PLT-02` は osx-x64 を配布対象に挙げているので、「配るが開発機としては支えない」非対称になる |

## nixpkgs の追従

`flake.lock` の更新は Renovate が追い、major 以外は automerge する（`DEC-PLT-033`）。
**major は PR にならない。** Dependency Dashboard に載るので、そこで着手を決める。
**落ちた PR は人が直す。** 直せば CI が緑になり、automerge が完走する。

**Playwright だけは制約がある。** `package.json` の版と nixpkgs の
`playwright-driver` の版が揃っていないと、入っていないブラウザを探して落ちる。
**nixpkgs が追いつくまで npm 側を上げない。** CI が両者を突き合わせて落とす。

**hash は機械が直す。** `flake.nix` の `fslc` と bun は SHA-256 で固定しているので、
版だけが上がると合わなくなる。上流が digest を公開しているので、
`.github/scripts/update-hashes.sh` が 6 個すべてを作り直す。CI では `nix-hash-fix` が
PR の中で当てる。**手で書き換えない。**

**版が上がって hash が古いと、黙って古いものを使う経路がある。** `fetchurl` の
同一性は hash だけで決まるので、`name` に版を入れていないと Nix が
「取得済み」と判断して古いバイナリを配る。だから `name` に版を入れてある。
外すと、更新したつもりで古い fslc が走る。

bun の版の正本は `package.json` の `packageManager`。 flake がそれを読むので、
flake 側に版を書かない。**Renovate はこの欄を触らない**（`bun` manager が
`npm` manager の更新を無効にする）ので、bun の版上げは人が始める。
