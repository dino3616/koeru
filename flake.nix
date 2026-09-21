# KOERU の開発環境。 Nix を第一級の前提とする（`DEC-PLT-033`）。
#
# ここが供給するのは「OS 固有の SDK 以外の開発ツール」。 Xcode Command Line Tools
# （macOS）と Windows の Build Tools はホスト側に残る——Tauri 自身が要求しており、
# Nix では置き換えられない。Windows は Nix の対象外なので、CI の windows ジョブが
# 唯一の検証経路として残る（`TR-PLT-01` は Windows を第一級の対象としている）。
#
# `inputs.self.submodules` は立てない。 立てると Kaldi（約123MB）と MFA の
# 音響モデルまで store へ複製される。ここは devShell であって、KOERU 自身を
# ビルドしない——submodule と LFS の正本は git のまま（`DEC-PLT-016`、`DEC-ALN-012`）。
{
  description = "KOERU の開発環境";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      # x86_64-darwin は席が無い。 nixpkgs 26.11 が対応を打ち切っており、
      # 評価そのものが throw する（26.05 なら通るが、そのために input を
      # 二重に持つ価値は無い）。fslc も macOS x64 のバイナリを出していない。
      #
      # `TR-PLT-02` は osx-x64 を配布対象に挙げているので、非対称が残る
      # ——Intel Mac 向けには配るが、Intel Mac は開発機として支えない。
      # Intel Mac の貢献者が現れたら判断を見直す（`DEC-PLT-033`）。
      systems = [
        "aarch64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems f;
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
    in
    {
      # `nix fmt` で整形する。 形式は RFC 166（nixfmt）。
      # Rust は `cargo fmt`、画面は `vp check`、ここは `nix fmt`——
      # 整形の契約を持たないファイルを作らない。CI が `--check` で通過を強制する。
      formatter = forAllSystems (system: (pkgsFor system).nixfmt);

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          inherit (pkgs) lib;

          # 画面側の package.json が bun と Playwright の版の正本。
          # ここで読むので、flake に版を書き写さない（`AGENTS.md` の禁止事項6）。
          ui = lib.importJSON ./crates/koeru-app/ui/package.json;

          # ---------------------------------------------------------------
          # bun
          #
          # nixpkgs の bun は使わない。 nixpkgs 側の版は nixpkgs リビジョンが決めるので、
          # `packageManager` と食い違う（実際に 1.3.13 と 1.4.2 で食い違っていた）。
          # 版は package.json から読み、ここに残るのは hash だけ——hash は版から
          # 機械的に導かれる事実なので、二重の主張にならない。
          #
          # hash は `nix/update-hashes.sh` が上流の `SHASUMS256.txt` から作り直す。
          # 手で書き換えない。 版上げのたびに必ず必要な機械的な更新なので、
          # `nix-hash-fix` ワークフローが PR の中で当てる（`DEC-PLT-033`）。
          #
          # Renovate は `packageManager` を触らない（`bun` manager が `npm` manager の
          # 更新を無効にする）ので、bun の版上げは人から始まる。
          # ---------------------------------------------------------------
          bunVersion = lib.removePrefix "bun@" ui.packageManager;
          bunAssets = {
            aarch64-darwin = {
              name = "bun-darwin-aarch64";
              hash = "sha256-kJh6OhbX21VtiGrD1VHnttPt8KHPQ6yu1iLoZ2vh0S8=";
            };
            aarch64-linux = {
              name = "bun-linux-aarch64";
              hash = "sha256-VDKLvC2cjgyfiSxUTWbFeoO4QTnjSQnl7oF1jxrI/ac=";
            };
            x86_64-linux = {
              name = "bun-linux-x64";
              hash = "sha256-NjaPrvdSeHXV/6UuU81IAhdB8qg+tiCKjdZAaNQiqRM=";
            };
          };
          bun =
            let
              asset = bunAssets.${system};
            in
            pkgs.stdenv.mkDerivation {
              pname = "bun";
              version = bunVersion;
              src = pkgs.fetchurl {
                # `name` に版を入れる。 既定は URL の basename で版が入らず、
                # fixed-output derivation の同一性は hash だけで決まる。
                # 版を上げて hash を直し忘れると、Nix が同じ store path を見て
                # 「取得済み」と判断し、**古いバイナリを黙って使う**。
                # 版を名前に入れておけば、取り違えではなく hash 不一致で落ちる。
                name = "bun-${bunVersion}-${asset.name}.zip";
                url = "https://github.com/oven-sh/bun/releases/download/bun-v${bunVersion}/${asset.name}.zip";
                inherit (asset) hash;
              };
              nativeBuildInputs = [
                pkgs.unzip
              ]
              ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.autoPatchelfHook ];
              buildInputs = lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.stdenv.cc.cc.lib ];
              dontConfigure = true;
              dontBuild = true;
              installPhase = ''
                runHook preInstall
                install -Dm755 bun "$out/bin/bun"
                ln -s "$out/bin/bun" "$out/bin/bunx"
                runHook postInstall
              '';
              meta.mainProgram = "bun";
            };

          # ---------------------------------------------------------------
          # fslc
          #
          # 版と SHA-256 の両方で固定する。 Renovate が追うのは版だけなので、
          # hash は `nix/update-hashes.sh` が上流の `.sha256` から作り直す。
          # 手で書き換えない（`DEC-PLT-033`）。
          # ---------------------------------------------------------------
          fslcVersion = "v4.6.0";
          fslcAssets = {
            aarch64-darwin = {
              name = "fslc-macos-arm64";
              hash = "sha256-KKPb138shsglN8DecMu1frWpd3XO3v68G22PC3f4Pbs=";
            };
            x86_64-linux = {
              name = "fslc-linux-x64";
              hash = "sha256-mLYKmRS8BiJal6XleapAmwTpyqpGjVnNOn6NqKqQKTQ=";
            };
            aarch64-linux = {
              name = "fslc-linux-arm64";
              hash = "sha256-IqQ65dkTbc6HommxVY4Sj9CnjE8wRkqPx1vKOQANdkM=";
            };
          };
          # system を足して資産を足し忘れたら、ここで評価が落ちる。
          # 退避経路は置かない——黙って fslc の無い devShell を配るより、
          # 組み立たないほうが良い（`bunAssets` も同じ形）。
          fslc =
            let
              asset = fslcAssets.${system};
            in
            pkgs.stdenv.mkDerivation {
              pname = "fslc";
              version = lib.removePrefix "v" fslcVersion;
              src = pkgs.fetchurl {
                # 版を名前に入れる理由は bun と同じ。
                name = "fslc-${fslcVersion}-${asset.name}";
                url = "https://github.com/ymm-oss/fsl/releases/download/${fslcVersion}/${asset.name}";
                inherit (asset) hash;
              };
              dontUnpack = true;
              dontConfigure = true;
              dontBuild = true;
              nativeBuildInputs = lib.optionals pkgs.stdenv.hostPlatform.isLinux [
                pkgs.autoPatchelfHook
              ];
              buildInputs = lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.stdenv.cc.cc.lib ];
              installPhase = ''
                runHook preInstall
                install -Dm755 "$src" "$out/bin/fslc"
                runHook postInstall
              '';
              meta.mainProgram = "fslc";
            };

          # story を実ブラウザで描いて axe を当てる（`TR-PLT-25`、`DEC-PLT-022`）。
          # npm 側の版と nixpkgs 側の版が揃っていないと、Playwright が
          # 入っていないブラウザを探して落ちる。整合は CI が検査する。
          playwrightVersion = ui.devDependencies.playwright;
        in
        {
          default = pkgs.mkShell {
            name = "koeru";

            packages = [
              (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
              bun
              fslc
              pkgs.cargo-deny
              pkgs.git
              # モデルの実体は LFS にある（`DEC-ALN-012`）。これが無いと
              # submodule の取得が途中で死ぬ。
              pkgs.git-lfs
              pkgs.pkg-config
              pkgs.playwright-driver.browsers
              # `flake.nix` を書くための道具。 編集機は PATH から拾うので、
              # 3台とも cloud とも同じ版になる。
              # nixd を採るのは、nixpkgs を実際に評価して補完するから
              # ——nil は静的解析だけで、パッケージ名が出てこない。
              pkgs.nixd
              pkgs.nixfmt
              # 反パターンと未使用の束縛。CI が強制する——走らせない検査は置かない。
              pkgs.statix
              pkgs.deadnix
            ]
            # Accelerate は SDK が持つ。`koeru-align` の build.rs が
            # `framework=Accelerate` を引く。
            ++ lib.optional pkgs.stdenv.hostPlatform.isDarwin pkgs.apple-sdk
            ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [
              # Tauri が Linux で要る一式（`DEC-PLT-014`）。
              # 以前は CI の composite action が apt で入れていた。ここが唯一の住所
              # ——2箇所に置くと、片方だけが古くなる（`AGENTS.md` の禁止事項6）。
              # 個別に削らない。 どれが要るかを自分で追うことになり、CI が1往復ずつ遅れる。
              pkgs.webkitgtk_4_1
              pkgs.libsoup_3
              pkgs.gtk3
              pkgs.libayatana-appindicator
              pkgs.librsvg
              pkgs.xdotool
              pkgs.openssl
              pkgs.patchelf
              pkgs.wrapGAppsHook3
            ];

            # ブラウザは Nix から出す。 ダウンロード版は NixOS と NixOS-WSL で
            # 動かない（FHS が無く、動的リンカに届かない）。
            PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
            PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
            PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";

            # 両方を出しておくと、CI が shell の中で突き合わせるだけで済む。
            # 揃っていないと Playwright が入っていないブラウザを探して落ちる。
            KOERU_PLAYWRIGHT_FROM_PACKAGE_JSON = playwrightVersion;
            KOERU_PLAYWRIGHT_FROM_NIXPKGS = pkgs.playwright-driver.version;

            shellHook = ''
              ${lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
                # 立てないと WebKitGTK が空白のウィンドウを描く。
                export WEBKIT_DISABLE_COMPOSITING_MODE=1
                # Wayland で表示倍率を正しく報告させる。
                export XDG_DATA_DIRS="$GSETTINGS_SCHEMAS_PATH:$XDG_DATA_DIRS"
              ''}

              if [ "$KOERU_PLAYWRIGHT_FROM_PACKAGE_JSON" != "$KOERU_PLAYWRIGHT_FROM_NIXPKGS" ]; then
                echo "警告: Playwright の版がずれている。" >&2
                echo "      package.json: $KOERU_PLAYWRIGHT_FROM_PACKAGE_JSON / nixpkgs: $KOERU_PLAYWRIGHT_FROM_NIXPKGS" >&2
                echo "      story の検査が落ちる。nixpkgs が追いつくまで npm 側を上げない。" >&2
              fi
            '';
          };
        }
      );
    };
}
