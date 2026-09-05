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
    for crate_name in ["koeru-core", "koeru-synth", "koeru-audio"] {
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
    for crate_name in ["koeru-core", "koeru-synth", "koeru-audio", "koeru-app"] {
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
    for crate_name in ["koeru-core", "koeru-synth", "koeru-audio", "koeru-app"] {
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
    let need = koeru_core::alias::required_aliases(Method::Single, &m, UnitSet::Core);
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

fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
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
const TRACE_FIELDS_ALLOWED: [&str; 38] = [
    "added_at",
    "bundled",
    "columns",
    "count",
    "device",
    // OS のデバイス識別子。`session.rs` が既に意図して記録している。
    // 弱い指紋にはなるので、`DEC-TEL-001` の review_trigger
    //（「ホワイトリストに載せたいフィールドが識別子として機能しうると判明したとき」）
    // に当たったら見直す。
    "device_id",
    "dither",
    "enc",
    // 書き出しの拡張子。固定の語彙。
    "ext",
    "files",
    "frames",
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

    for crate_name in [
        "koeru-core",
        "koeru-synth",
        "koeru-audio",
        "koeru-app",
        "koeru-align",
    ] {
        let src = root.join("crates").join(crate_name).join("src");
        walk(&src, &mut |path, text| {
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                if !line.contains("tracing::instrument") {
                    continue;
                }
                // 属性から関数シグネチャまでを1つに畳む。
                let mut attr = (*line).to_owned();
                let mut sig = String::new();
                for probe in lines.iter().skip(i + 1).take(6) {
                    if probe.contains("fn ") {
                        sig = (*probe).to_owned();
                        break;
                    }
                    attr.push_str(probe.trim());
                }
                if sig.is_empty() {
                    continue;
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
    println!(
        "instrument の記録フィールドは許可リスト {} 語に収まっている",
        TRACE_FIELDS_ALLOWED.len()
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
