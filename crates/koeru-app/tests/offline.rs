//! 試唱の全経路がオフラインで動くことを確かめる（`TR-SYN-32`, `TR-PLT-20`）。
//!
//! 直接ネットワークを止めて試すことはできないので、そもそも外へ出る経路が
//! コードに無いことと、同梱物だけで動くことの2つで確かめる。
//!
//! # 何を見ているか
//!
//! 1. 依存の木に HTTP クライアントが入っていない
//! 2. ソースに外部プロセスの起動が無い（`TR-SYN-01`。外部プロセスの起動もネットワーク通信も禁じている）
//! 3. 合成コア・phonemizer・録音リストが、同梱物だけで動く

// 実機ハーネスなので `println!` を通す。 ここは人が読む出力で、
// 走らせた本人が数値を見て判断する。`tracing` へ出すと、
// 既定のフィルタでは見えず、走らせた意味が無くなる。
#![allow(clippy::print_stdout)]

use std::path::Path;

/// 引いてはいけない crate。
const NETWORK_CRATES: [&str; 9] = [
    "reqwest",
    "hyper",
    "ureq",
    "curl",
    "isahc",
    "attohttpc",
    "surf",
    "tokio-tungstenite",
    "tungstenite",
];

/// 合成の経路は HTTP クライアントを引かない（`TR-SYN-32`）。
///
/// `koeru-app` は Tauri を引き、Tauri は `reqwest` を引く（asset protocol のため）。
/// これは KOERU が通信することを意味しない。 止めるべきなのは
/// 「合成・録音・ドメインの経路が外へ出ること」なので、そこを見る。
#[test]
fn 合成の経路がhttpクライアントを引かない() {
    let root = repo_root();
    for crate_name in [
        "koeru-model",
        "koeru-formats",
        "koeru-core",
        "koeru-synth",
        "koeru-audio",
    ] {
        let manifest = root.join("crates").join(crate_name).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).expect("読めること");
        for name in NETWORK_CRATES {
            assert!(
                !text.contains(name),
                "{crate_name} が {name} を引いている。 処理はローカル完結で、声をサーバへ送らない"
            );
        }
    }
    println!("合成・録音・ドメインの経路に HTTP クライアントは無い");
}

/// KOERU 自身のコードが HTTP クライアントを使わない（`TR-SYN-32`, `TR-PLT-20`）。
///
/// Tauri が引いているものを、こちらから呼ばない。
#[test]
fn 自分のコードがhttpクライアントを呼ばない() {
    let root = repo_root();
    let mut found = Vec::new();
    for crate_name in [
        "koeru-model",
        "koeru-formats",
        "koeru-core",
        "koeru-synth",
        "koeru-audio",
        "koeru-app",
    ] {
        let src = root.join("crates").join(crate_name).join("src");
        walk(&src, &mut |path, text| {
            for name in NETWORK_CRATES {
                let ident = name.replace('-', "_");
                if text.contains(&format!("{ident}::")) {
                    found.push(format!("{} が {name} を呼んでいる", path.display()));
                }
            }
        });
    }
    assert!(found.is_empty(), "{found:?}");
    println!("自分のコードは HTTP クライアントを呼んでいない");
}

/// 初回起動時のダウンロードを行わない（`TR-SYN-32`, `TR-PLT-20`）。
///
/// 配るものの中に、外を指す設定が無いことを見る。
#[test]
fn 設定が外を指していない() {
    let root = repo_root();
    let conf =
        std::fs::read_to_string(root.join("crates/koeru-app/tauri.conf.json")).expect("読めること");

    // フロントは同梱したファイルから読む。 リモートを指さない。
    assert!(
        conf.contains("\"frontendDist\": \"ui/dist/client\""),
        "配るフロントが同梱物であること"
    );
    // 更新機構を持たない。 持つと、起動のたびに外へ出る。
    assert!(!conf.contains("updater"), "更新機構を持たないこと");
    // 開発用の口はローカルだけ。
    assert!(
        !conf.contains("devUrl") || conf.contains("http://localhost:1420"),
        "開発用の口がローカルであること"
    );
    // CSP。 ここが WebView を外へ出さない壁で、`core:default` の権限では塞げない。
    // 権限は Rust 側のコマンドを絞るだけで、画面の JS が fetch することは止めない。
    let csp = conf
        .lines()
        .find(|l| l.contains("\"csp\""))
        // CSP の中にも `:` が居る（`data:` `blob:`）。最初の1つでだけ切る。
        .and_then(|l| l.split_once(':').map(|x| x.1))
        .map(|v| v.trim().trim_matches(['"', ',']).to_owned())
        .expect("CSP が書かれていること");

    // 送信先を決める2つは `'self'` から始まる。
    for directive in ["default-src 'self'", "connect-src 'self'"] {
        assert!(
            csp.contains(directive),
            "CSP に `{directive}` があること: {csp}"
        );
    }

    // 出どころとして許すのはこれだけ。 `*.localhost` は Tauri 自身の口
    // （`asset:` と `ipc:` の HTTP 版）で、外へは出ない。
    const ALLOWED: [&str; 6] = [
        "'self'",
        "'unsafe-inline'",
        "data:",
        "blob:",
        "asset:",
        "ipc:",
    ];
    for part in csp.split(';') {
        // 先頭はディレクティブ名。残りが出どころ。
        for src in part.split_whitespace().skip(1) {
            let ok = ALLOWED.contains(&src)
                || src
                    .strip_prefix("http://")
                    .is_some_and(|h| h.ends_with(".localhost"));
            assert!(ok, "CSP が外部を指している（`{src}`）: {csp}");
        }
    }
}

/// 外部プロセスを起動しない（`TR-SYN-01`）。
///
/// `.exe`、Wine、Python インタプリタのいずれも起動しない。
///
/// 例外は1つだけ（`TR-SYN-35`）。本人が明示的に指した resampler を呼ぶ経路。
/// そこに閉じていることを、この検査が保つ。 他へ広がったら落ちる。
const EXTERNAL_PROCESS_ALLOWED: [&str; 1] = ["external.rs"];

#[test]
fn 合成の経路に外部プロセスの起動が無い() {
    let root = repo_root();
    let mut found = Vec::new();
    for crate_name in [
        "koeru-model",
        "koeru-formats",
        "koeru-core",
        "koeru-synth",
        "koeru-audio",
        "koeru-app",
    ] {
        let src = root.join("crates").join(crate_name).join("src");
        walk(&src, &mut |path, text| {
            let allowed = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| EXTERNAL_PROCESS_ALLOWED.contains(&n));
            if !allowed && text.contains("process::Command") {
                found.push(path.display().to_string());
            }
        });
    }
    assert!(
        found.is_empty(),
        "外部プロセスの起動が見つかった: {found:?}"
    );
    println!(
        "外部プロセスの起動は {:?} だけに閉じている",
        EXTERNAL_PROCESS_ALLOWED
    );
}

/// 同梱物だけで、録音リストと課題曲と phonemizer が動く（`TR-SYN-32`, `TR-PLT-20`）。
///
/// 初回起動時のダウンロードを行わない。
#[test]
fn 同梱物だけで一通り動く() {
    use koeru_core::alias::Method;
    use koeru_core::inventory::UnitSet;
    use koeru_core::{mora, reclist, ust};

    // 録音リスト。第三者の録音リストファイルを同梱しない（`TR-RCL-02`）。
    let list = reclist::generate_single(UnitSet::Core, 5).expect("生成できること");
    assert!(!list.is_empty());

    // 課題曲。同梱はパブリックドメインの伝承曲だけ（`TR-RCL-12`）。
    let songs = ust::bundled_songs();
    assert_eq!(songs.len(), 1);

    // phonemizer。辞書を外から取らない（`TR-SYN-11`）。
    let m = mora::parse("さくらさくら", UnitSet::Core).expect("読めること");
    let need = koeru_core::alias::required_aliases(
        &koeru_core::presamp::Rules::builtin(UnitSet::Core),
        Method::Single,
        &m,
        &std::collections::BTreeSet::new(),
    );
    assert!(!need.is_empty());

    // 合成コア。同梱した WORLD（`TR-SYN-05`）。
    let x: Vec<f64> = (0..4410)
        .map(|i| (2.0 * std::f64::consts::PI * 220.0 * f64::from(i) / 44_100.0).sin() * 0.5)
        .collect();
    let cond = koeru_synth::f0::conditions(koeru_synth::f0::Purpose::Preview, None);
    let (f0, _) = koeru_synth::f0::estimate(&x, 44_100, &cond);
    assert!(!f0.is_empty(), "同梱した合成コアが動くこと");

    println!(
        "録音リスト {} 行 / 課題曲 {} 曲 / 必要単位 {} 個",
        list.len(),
        songs.len(),
        need.len()
    );
}

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("リポジトリの根があること")
}

/// `.rs` を全部たどる。
///
/// **読めないディレクトリで黙って戻っていた。** 走査する先を打ち間違えると、
/// 1ファイルも見ずに検査が通る。読めなければ落とす（`DEC-PLT-039`）。
fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{} を読めない: {e}", dir.display()));
    for e in entries.filter_map(Result::ok) {
        let p = e.path();
        if p.is_dir() {
            walk(&p, f);
        } else if p.extension().is_some_and(|x| x == "rs")
            && let Ok(text) = std::fs::read_to_string(&p)
        {
            f(&p, &text);
        }
    }
}

/// トレースに載せてよいフィールド名（`TR-PKG-53`〜`TR-PKG-56`、`DEC-TEL-001`）。
///
/// 数・寸法・列挙・ID だけ。自由文を入れない。
/// 音源名・ファイルパス・歌詞・プロジェクト名が入ると、
/// 「非公開のまま完成できる」という製品の前提が崩れる。
const TRACE_FIELDS_ALLOWED: &[&str] = &[
    // 失敗の記録（`DEC-PLT-038`）。 どれも `koeru-failure` の固定の語彙で、
    // `phase` も工程を指す固定の名前。画面から来た値を入れない。
    "code",
    "class",
    "outcome",
    "phase",
    // 検査が複数行の event と省略形を読めるようになって見つかったもの。
    // どれも件数・寸法・真偽・レート・OSStatus・固定の語彙で、本人のものではない。
    "attempt",
    "budget_ms",
    "bytes",
    // 試唱の待ち時間の区分（`latency::Case`）。固定の列挙。
    "case",
    "ch",
    "converting",
    "correlation",
    "device_rate_hz",
    "elapsed_ms",
    "entries",
    "got",
    "held",
    "held_ms",
    "lag_ms",
    "leaking",
    "master_rate_hz",
    "max_frames",
    "may_mix",
    "next_gain",
    "presamp",
    "rate",
    // リサンプラの識別子（`koeru_audio::resample::IDENTIFIER`）。定数。
    "resampler",
    "rms",
    "singable",
    "songs",
    "source_channel",
    "status",
    "subbanks",
    "want",
    "want_ms",
    "added_at",
    // ここから下は、値そのものが本人のものではないもの。
    // 数・レート・固定の語彙で、識別にも復元にも使えない。
    "base_name",
    "ceil_hz",
    "cfg",
    "discontinuities",
    "frame_period_ms",
    "from_ms",
    "gates",
    "guide_offset_frames",
    "method",
    // 録る順のモード（`TR-SYN-19`）。SongBankFirst か CoverageEfficiency の2語。
    "mode",
    "preroll_frames",
    // 方式プリセットの ID（`TR-RCL-01`）。同梱の固定語彙で、本人のものではない。
    "preset_id",
    // 書き出し方（`TR-PKG-12`）。classic / openutau / both の3語しかない。
    "profile",
    "sample_rate_hz",
    "to_ms",
    // 自動移調の量（`TR-SYN-15`）。-12 のような半音の数。
    "semitones",
    // 収録音高の本数（`TR-REC-25`）。1 か 3 のような数で、音高そのものではない。
    "tones",
    // 録り直しで固定した左ブランクを当て直した件数と、当て直せずに絶対位置のまま
    // 写した件数（`DEC-ALN-019`）。数だけ。
    "shifted",
    "absolute",
    "bundled",
    "columns",
    "count",
    "device",
    // OS のデバイス識別子。`session.rs` が既に意図して記録している。
    // 弱い指紋にはなるので、`DEC-TEL-001` の review_trigger
    //（「ホワイトリストに載せたいフィールドが識別子として機能しうると判明したとき」）
    // に当たったら見直す。
    "device_id",
    "dim",
    // 特徴の次元数。40 のような数。モデルの形であって本人のものではない。
    "dither",
    "effects",
    // OS 側の効果の列挙結果（`TR-REC-08`）。固定の種別が並ぶだけ。
    "enc",
    // 書き出しの拡張子。固定の語彙。
    "ext",
    "files",
    "floor_hz",
    // 探索の下限（Hz）。話者音域から決まる数で、声そのものではない。
    "found",
    // 見つかった件数。数だけ。
    "frames",
    // ライブラリを置いたファイルシステムの種類（`storage::FsKind::as_str`、
    // `DEC-PKG-016`）。固定の語彙で、置き場所そのものではない。
    "fs_kind",
    "id",
    "in_bank",
    "index",
    "kind",
    // スナップショットの契機。`realign` / `bulk_alias` などの固定語彙で、利用者の文字列ではない。
    "label",
    "len",
    "len_ms",
    "length_ms",
    "measured_at",
    "midi",
    "ms",
    "notes",
    "out_len",
    "per_row",
    "pixels",
    "rate_hz",
    "reason",
    // 打ち切りや失敗の理由。`as_str` / `kind()` が返す固定語に限る。
    // ライブラリの置き場所の移し替えの結果（`relocate::Relocation::as_str`、
    // `DEC-PKG-016`）。固定の語彙で、パスそのものではない。
    "relocation",
    "ring_capacity",
    "row",
    // 同梱の録音リストの行を指す。利用者の創作物ではないので載せてよい
    // （`db.rs` の `fields(row = %t.row_id)` が既にそう扱っている）。
    "row_id",
    "rows",
    "seconds",
    "seq",
    "set",
    "take_id",
    "takes",
    "tone",
    "value",
];

/// `#[tracing::instrument]` が記録するフィールド名が、許可リストに収まっている。
///
/// `#[instrument]` は `skip()` に入れなかった引数を全部記録する。
/// 除外し忘れると、音源名やパスがそのままスパンに載る。
/// 規約（`AGENTS.md` の禁止事項3）は許可リスト方式を要求しているので、
/// 入れ忘れが起きたらここで落とす。
///
/// `tests/offline.rs` の他の検査と同じく、ソースを走査して構造を固定する。
#[test]
fn トレースのフィールドが許可リストに収まっている() {
    let root = repo_root();
    let mut leaked = Vec::new();
    let (mut files, mut events, mut spans) = (0_usize, 0_usize, 0_usize);

    // 全クレートを見る。 **足し忘れると、その crate だけ素通りする。**
    // `koeru-package` を足したとき、`verify` が配布名と `install.txt` の
    // バイト列をそのままスパンへ載せていた。**踏んだ。** 名前を並べると同じことが
    // 起きるので、`crates/` の下にあるものを全部読む。
    let crates = workspace_crates(&root);
    assert!(
        crates.len() >= 8,
        "crate が少なすぎる。読み方が壊れていないか: {crates:?}"
    );
    for crate_name in &crates {
        let src = root.join("crates").join(crate_name).join("src");
        walk(&src, &mut |path, text| {
            files += 1;
            // `info!(reason = …)` のように、イベントへ直接付けたフィールド。
            //
            // `#[instrument]` だけ見ていた頃は、この経路が丸ごと素通りだった。
            // 実際に `device = ?id` がデバイス識別子を載せていた。
            for (line_no, name) in event_fields(text) {
                events += 1;
                if !TRACE_FIELDS_ALLOWED.contains(&name.as_str()) {
                    leaked.push(format!("{}:{line_no}: イベントの `{name}`", path.display()));
                }
            }

            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                if !line.contains("tracing::instrument") || line.trim_start().starts_with("//") {
                    continue;
                }
                spans += 1;
                // 属性から関数シグネチャまでを1つに畳む。
                //
                // **シグネチャは閉じ括弧まで読む。** 1行目だけを見ていたので、
                // 引数を改行して並べた関数は「引数が無い」ものとして素通りしていた
                // ——`koeru-package` の `verify` が、配布名と `install.txt` の
                // バイト列をそのままスパンへ載せていた。**踏んだ。**
                let mut attr = (*line).to_owned();
                let mut sig = String::new();
                let mut in_sig = false;
                for probe in lines.iter().skip(i + 1).take(24) {
                    if !in_sig && !probe.contains("fn ") {
                        attr.push_str(probe.trim());
                        continue;
                    }
                    in_sig = true;
                    sig.push_str(probe.trim());
                    // 括弧が閉じたら終わり。戻り値の `->` までは要らない。
                    let opens = sig.matches('(').count();
                    let closes = sig.matches(')').count();
                    if opens > 0 && opens == closes {
                        break;
                    }
                }
                if sig.is_empty() {
                    continue;
                }

                let attr_args = attr
                    .split_once("instrument(")
                    .map(|(_, rest)| top_level_args(rest))
                    .unwrap_or_default();
                // `err` と `ret` は失敗や戻り値の `Display` を段ごとに記録する。
                // 失敗は持ち主が1回だけ、型つきの event で出す（`DEC-PLT-038`）。
                for arg in &attr_args {
                    let arg = arg.trim();
                    if ["err", "ret"]
                        .iter()
                        .any(|k| arg == *k || arg.starts_with(&format!("{k}(")))
                    {
                        leaked.push(format!(
                            "{}:{} の instrument が `{arg}` を持つ",
                            path.display(),
                            i + 1
                        ));
                    }
                }
                // `fields(...)` に書いた名前も、スパンに載る。
                for arg in attr_args.iter().filter(|a| a.trim().starts_with("fields(")) {
                    let inner = &arg.trim()["fields(".len()..];
                    for name in top_level_args(inner)
                        .iter()
                        .filter_map(|f| field_name(f.trim()))
                    {
                        if !TRACE_FIELDS_ALLOWED.contains(&name.as_str()) {
                            leaked.push(format!(
                                "{}:{} の instrument の fields に `{name}`",
                                path.display(),
                                i + 1
                            ));
                        }
                    }
                }

                if attr.contains("skip_all") {
                    continue;
                }

                let skipped: Vec<String> = attr
                    .split_once("skip(")
                    .and_then(|(_, rest)| rest.split_once(')'))
                    .map(|(inner, _)| inner.split(',').map(|s| s.trim().to_owned()).collect())
                    .unwrap_or_default();

                // 引数名を拾う。`self` と skip 済みは対象外。
                //
                // ジェネリクスの `,` と `::` で切ると壊れるので、深さを見て切る。
                let Some((_, args)) = sig.split_once('(') else {
                    continue;
                };
                let mut depth = 0i32;
                let mut parts: Vec<String> = Vec::new();
                let mut cur = String::new();
                for ch in args.chars() {
                    match ch {
                        '<' | '(' | '[' => {
                            depth += 1;
                            cur.push(ch);
                        }
                        '>' | ']' => {
                            depth -= 1;
                            cur.push(ch);
                        }
                        ')' if depth == 0 => break,
                        ')' => {
                            depth -= 1;
                            cur.push(ch);
                        }
                        ',' if depth == 0 => {
                            parts.push(std::mem::take(&mut cur));
                        }
                        _ => cur.push(ch),
                    }
                }
                parts.push(cur);

                for part in parts {
                    // `name: Type` の `name` だけを採る。`::` は型側なので数えない。
                    let Some(colon) = part.find(':') else {
                        continue;
                    };
                    if part[colon..].starts_with("::") {
                        continue;
                    }
                    let name = part[..colon].trim().trim_start_matches("mut ").trim();
                    if name.is_empty()
                        || name == "self"
                        || name == "&self"
                        || name == "&mut self"
                        || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
                    {
                        continue;
                    }
                    if skipped.iter().any(|s| s == name) {
                        continue;
                    }
                    if TRACE_FIELDS_ALLOWED.contains(&name) {
                        continue;
                    }
                    leaked.push(format!(
                        "{}:{} の引数 `{}` が skip されておらず、許可リストにも無い",
                        path.display(),
                        i + 1,
                        name
                    ));
                }
            }
        });
    }

    assert!(
        leaked.is_empty(),
        "トレースに載ってはいけない値がスパンへ入る:\n  {}",
        leaked.join("\n  ")
    );
    // 0件で通らないようにする。 走査の先を打ち間違えても、件数で気づく（`DEC-PLT-039`）。
    assert!(
        files > 80 && events > 40 && spans > 100,
        "走査したものが少なすぎる（ファイル {files} / event のフィールド {events} / instrument {spans}）"
    );
    println!(
        "ファイル {files} 個の event のフィールド {events} 件と instrument {spans} 個が、許可リスト {} 語に収まっている",
        TRACE_FIELDS_ALLOWED.len()
    );
}

/// 走査する crate。 `crates/` の下で `Cargo.toml` を持つディレクトリ全部。
fn workspace_crates(root: &Path) -> Vec<String> {
    let dir = root.join("crates");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} を読めない: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| e.path().join("Cargo.toml").is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort();
    names
}

/// 検査そのものを検査する（`DEC-PLT-039`）。 読み方が壊れると、上の検査は黙って通る。
#[test]
fn トレースの読み方が改行と省略形を拾う() {
    let src = "tracing::info!(\n    device_rate_hz,\n    reason = e.code(),\n    %path,\n    \"始める {}\", x\n);\n// tracing::warn!(commented = 1)\n";
    let names: Vec<String> = event_fields(src).into_iter().map(|(_, n)| n).collect();
    assert_eq!(names, ["device_rate_hz", "reason", "path"]);
    assert_eq!(field_name("a == b"), None);
    assert_eq!(
        top_level_args("skip(self), fields(n = f(a, b)), err)"),
        ["skip(self)", " fields(n = f(a, b))", " err"]
    );
}

#[test]
fn 画面のクリップ閾値がrustと一致する() {
    let ts = std::fs::read_to_string(repo_root().join("crates/koeru-app/ui/src/lib/levels.ts"))
        .expect("levels.ts が読めない");

    let line = ts
        .lines()
        .find(|l| l.starts_with("export const CLIP_THRESHOLD"))
        .expect("levels.ts に CLIP_THRESHOLD の定義が無い");
    let value: f32 = line
        .split('=')
        .nth(1)
        .and_then(|v| v.trim().trim_end_matches(';').parse().ok())
        .unwrap_or_else(|| panic!("CLIP_THRESHOLD の値が読めない: {line}"));

    assert!(
        (value - koeru_core::analysis::CLIP_THRESHOLD).abs() < f32::EPSILON,
        "levels.ts の CLIP_THRESHOLD が {value}、Rust は {}。片方だけ変わっている",
        koeru_core::analysis::CLIP_THRESHOLD,
    );
}

/// 境界の enum が、バックエンドの文字列を取りこぼしていない。
///
/// [`koeru_app_lib::commands`] の `parse` は、知らない文字列を
/// `Unknown` / `Unavailable` へ落とす。 落ちても型は通り、テストも通り、
/// 画面には「判定できません」と出るだけ——`TR-REC-11` の警告が黙って消える。
///
/// バックエンドの `as_str` が返す綴りを直接読んで、どれかが取りこぼされて
/// いないかを見る。 変種を数え上げられないので、原文を当たっている。
#[test]
fn 境界のenumがバックエンドの綴りを網羅している() {
    // (バックエンドのファイル, その enum, 境界側が知っている綴り)
    let cases: [(&str, &str, &[&str]); 4] = [
        (
            "crates/koeru-audio/src/backend/macos/capture_device.rs",
            "MicrophoneMode",
            &["Standard", "VoiceIsolation", "WideSpectrum", "Unknown"],
        ),
        (
            "crates/koeru-audio/src/backend/macos/gain.rs",
            "GainControl",
            &["hardware", "software", "unavailable"],
        ),
        (
            "crates/koeru-audio/src/backend/macos/output.rs",
            "OutputKind",
            &["headphones", "speakers", "unknown"],
        ),
        (
            "crates/koeru-audio/src/backend/unsupported.rs",
            "MicrophoneMode / GainControl / OutputKind",
            &["Unknown", "unavailable", "unknown"],
        ),
    ];

    for (path, what, known) in cases {
        let src = std::fs::read_to_string(repo_root().join(path))
            .unwrap_or_else(|_| panic!("{path} が読めない"));

        for spelled in as_str_literals(&src) {
            assert!(
                known.contains(&spelled.as_str()),
                "{path} の {what} が `{spelled}` を返すのに、\
                 commands.rs の parse がそれを知らない。\
                 知らない綴りは Unknown / Unavailable へ落ちるので、\
                 画面からは判定できなかったのと見分けが付かない",
            );
        }
    }
}

/// イベントマクロに直接書かれたフィールド名を、書かれた行とともに拾う。
///
/// `info!(reason = e.code(), "…")` の `reason`。 名前だけを見る——
/// 値が何であれ、許可リストに無い名前は送信層へ載せない（禁止事項3）。
///
/// `%` と `?` の前置きも同じ扱い。`?id` は `Debug` を載せる形なので、
/// むしろ危ないほうに入る。 `warn!(discontinuities, "…")` のように名前だけを書く
/// 省略形も、同じ名前のフィールドになる。
///
/// **マクロの1行目しか読んでいなかった。** 引数を改行して並べた event と、
/// 省略形のフィールドは、どちらも素通りしていた。 括弧の深さと文字列を見て
/// 引数を切り、最初の文字列（文言）より前だけをフィールドとして読む。
fn event_fields(text: &str) -> Vec<(usize, String)> {
    const MACROS: [&str; 5] = ["info!(", "warn!(", "error!(", "debug!(", "trace!("];
    let mut out = Vec::new();
    for (start, _) in text.match_indices('!') {
        let Some(m) = MACROS.iter().find(|m| {
            start + 2 >= m.len() && text.get(start + 2 - m.len()..start + 2) == Some(**m)
        }) else {
            continue;
        };
        let begin = start + 2 - m.len();
        let line_no = text[..begin].matches('\n').count() + 1;
        let line = text[..begin].rsplit('\n').next().unwrap_or("");
        // コメントの中の例は対象外。
        if line.contains("//") {
            continue;
        }
        for arg in top_level_args(&text[start + 2..]) {
            let arg = arg.trim();
            // 文言より後ろは書式の引数。フィールドではない。
            if arg.starts_with('"') {
                break;
            }
            if let Some(name) = field_name(arg) {
                out.push((line_no, name));
            }
        }
    }
    out
}

/// 開き括弧の直後から、対応する閉じ括弧までを、深さ 0 の `,` で切る。
fn top_level_args(rest: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut depth = 0_i32;
    let mut chars = rest.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                cur.push(ch);
                while let Some(c) = chars.next() {
                    cur.push(c);
                    if c == '\\' {
                        if let Some(esc) = chars.next() {
                            cur.push(esc);
                        }
                    } else if c == '"' {
                        break;
                    }
                }
            }
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(ch);
            }
            ')' | ']' | '}' if depth == 0 => break,
            ')' | ']' | '}' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        parts.push(cur);
    }
    parts
}

/// `name = 値` / `%name` / `?name` / `name` のフィールド名。 `target:` などの指定は除く。
fn field_name(arg: &str) -> Option<String> {
    if ["target:", "parent:", "name:"]
        .iter()
        .any(|k| arg.starts_with(k))
    {
        return None;
    }
    let lhs = match arg.split_once('=') {
        // `==` や `>=` は比較。フィールドではない。
        Some((l, r)) if !r.starts_with('=') && !l.ends_with(['!', '<', '>']) => l,
        Some(_) => return None,
        None => arg,
    };
    let name = lhs.trim().trim_start_matches(['%', '?']).trim();
    (!name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        && !name.starts_with(|c: char| c.is_ascii_digit()))
    .then(|| name.to_owned())
}

/// `as_str` の本体に現れる文字列リテラルを拾う。
///
/// `pub const fn as_str` から、その関数を閉じる `}` までを見る。
/// ファイル全体を見ると、無関係な文字列まで拾ってしまう。
fn as_str_literals(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        if !lines[i].contains("fn as_str") {
            i += 1;
            continue;
        }
        let indent = lines[i].len() - lines[i].trim_start().len();
        let close = format!("{}}}", " ".repeat(indent));
        i += 1;
        while i < lines.len() && lines[i] != close {
            let mut rest = lines[i];
            while let Some(a) = rest.find('"') {
                let after = &rest[a + 1..];
                let Some(b) = after.find('"') else { break };
                out.push(after[..b].to_owned());
                rest = &after[b + 1..];
            }
            i += 1;
        }
    }
    out
}
