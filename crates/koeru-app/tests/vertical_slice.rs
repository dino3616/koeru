//! **縦切りを1本通す実機ハーネス。**
//!
//! プロジェクトを作る → デバイスを開く → 1行録る → 確定 → 解析 → `.frq` →
//! 境界 → oto → 目標音高で合成 → 鳴らす。
//!
//! **これは回帰テストではない。** マイクが無い環境では途中で戻る。
//! 何が起きたかを読むために標準出力を使う。

#![allow(clippy::print_stdout)]
// **macOS 専用。** 他 OS のバックエンドはまだ無い（DEC-REC-001）。
#![cfg(target_os = "macos")]

use koeru_app_lib::Studio;
use koeru_audio::backend::macos as mac;

/// 収録する長さ。**短くする。** 通ることを確かめるのが目的。
const RECORD_MS: u64 = 900;

/// これを下回る素材は、信号ではなく部屋の音。
const SIGNAL_FLOOR: f32 = 0.05;

/// これを下回る境界は、発声ではなく物音。
///
/// **ピークだけでは足りない。** ドアや打鍵は大きいが声ではなく、
/// そこから測った F0 は推定器の出まかせになる（実機で踏んだ）。
const CONFIDENCE_FLOOR: f64 = 0.5;

#[test]
fn 録って聴けるところまで一本で通す() {
    let root = std::env::temp_dir().join(format!("koeru-slice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
    let id = studio
        .create_project("試験用の音源")
        .expect("プロジェクトを作れる");
    studio.open_project(id).expect("開ける");

    let before = studio.progress().expect("進み具合を引ける");
    println!(
        "録音リスト: {} 項目, 次は {:?}",
        before.required, before.next_row
    );
    assert!(before.required > 0, "必要な単位があること");
    assert!(before.next_row.is_some(), "次に録る行があること");

    // **権限が無いと macOS は無音を返す**（TR-REC-17）。
    // ここを見ないと「マイクが壊れている」と読み違える。
    let perm = mac::microphone_permission();
    println!("マイク権限: {perm:?}");
    if !matches!(perm, mac::MicPermission::Granted) {
        println!(
            "**この実行ファイルにマイク権限が無い。** \
             cargo test のバイナリは毎回パスが変わるので、TCC の許可が引き継がれない。\n\
             設定 → プライバシーとセキュリティ → マイク で許可すると通る: {}",
            mac::privacy_settings_url()
        );
    }

    // ── 信号が届いているデバイスを選ぶ ──
    // **既定が無音のことがある**（実機で踏んだ）。
    let devices = Studio::devices().expect("デバイスを挙げられる");
    let mut chosen = None;
    let mut best = 1e-6_f32;
    for d in &devices {
        if let Err(e) = studio.arm_device(&d.id) {
            println!("  候補 {:?}: 開けない（{}）", d.id, e.kind);
            continue;
        }
        let peak = studio.probe_input(250).unwrap_or(0.0);
        println!("  候補 {:?}: ピーク {peak:.6}", d.id);
        // **一番よく聞こえているものを採る。** 最初に閾値を超えたもので決めると、
        // わずかに乗っているだけのデバイスを掴んで、ほぼ無音を録ることになる（踏んだ）。
        if peak > best {
            best = peak;
            chosen = Some(d.id.clone());
        }
    }
    let Some(device) = chosen else {
        println!("入力が届くデバイスが無い。ここで戻る");
        return;
    };
    studio.arm_device(&device).expect("開き直せる");
    studio.probe_input(200).expect("生死を判定できる");

    // **プリロールが溜まるまで待つ**（TR-REC-19）。
    // 収録画面に入った直後は、まだ遡れる分が無い。
    std::thread::sleep(std::time::Duration::from_millis(
        koeru_app_lib::pump::PREROLL_MS + 200,
    ));
    let held = studio.preroll_ms();
    println!(
        "プリロール: {held}ms（要 {}ms）",
        koeru_app_lib::pump::PREROLL_MS
    );
    assert!(
        held >= koeru_app_lib::pump::PREROLL_MS,
        "収録画面にいる間ストリームが止まっていないこと"
    );

    // ── 1行録る ──
    let row = studio.start_take().expect("収録を始められる");
    println!("収録中: {row}");
    std::thread::sleep(std::time::Duration::from_millis(RECORD_MS));
    let take = studio.finish_take().expect("確定できる");

    println!(
        "確定: take={} row={} 長さ={:.0}ms ピーク={:.4} 取りこぼし={} 無効化={}",
        take.take_id,
        take.row_id,
        take.duration_ms,
        take.peak,
        take.discontinuities,
        take.invalidated
    );
    println!(
        "  プリロール {:.0}ms / ピーク {:.1}dBFS / RMS {:.5} / フルスケール {} 回 / DC {:+.5}",
        take.preroll_ms,
        take.metrics.peak_dbfs,
        take.metrics.rms,
        take.metrics.full_scale_runs,
        take.metrics.dc_offset
    );
    println!(
        "  無音マージン 前 {:.0}ms / 後 {:.0}ms（要 {:.0}ms 以上）→ {}",
        take.metrics.leading_margin_ms,
        take.metrics.trailing_margin_ms,
        koeru_core::analysis::REQUIRED_MARGIN_MS,
        if take.metrics.has_required_margins() {
            "足りている"
        } else {
            "足りていない"
        }
    );
    assert_eq!(take.row_id, row);
    assert_eq!(
        take.thumbnail.len(),
        koeru_core::analysis::THUMBNAIL_BUCKETS
    );

    // **押した瞬間より前から録れていること**（TR-REC-19）。
    let want_preroll = koeru_app_lib::pump::PREROLL_MS as f64;
    assert!(
        (take.preroll_ms - want_preroll).abs() < 30.0,
        "遡った長さが {want_preroll}ms 付近であること: {:.0}ms",
        take.preroll_ms
    );

    // 全体は「遡り + 指示の間 + 末尾の延長」。**指示の長さより必ず長い。**
    let want_total = want_preroll + RECORD_MS as f64 + koeru_app_lib::pump::TAIL_MS as f64;
    assert!(
        take.duration_ms > RECORD_MS as f64,
        "指示の長さより長いこと（前後に足されている）"
    );
    assert!(
        (take.duration_ms - want_total).abs() < 150.0,
        "遡り + 指示 + 末尾 の合計付近であること（想定 {want_total:.0}ms、実測 {:.0}ms）",
        take.duration_ms
    );

    // **取りこぼしたテイクは自動的に無効になる**（TR-REC-07）。
    assert_eq!(
        take.invalidated,
        take.discontinuities > 0,
        "無効化は取りこぼしと1対1であること"
    );

    // ── ファイルが実際にあること（DEC-REC-004 の順序）──
    let dir = studio.project_dir().expect("ディレクトリを引ける").clone();
    let wavs: Vec<_> = std::fs::read_dir(dir.audio_dir())
        .expect("audio を読める")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    println!("audio/: {wavs:?}");
    assert!(wavs.iter().any(|n| n.ends_with(".wav")), "WAV が残ること");
    assert!(
        wavs.iter().any(|n| n.ends_with(".frq")),
        "**周波数表を録音停止時に書くこと**（TR-PKG-05）"
    );
    assert!(
        !wavs.iter().any(|n| n.ends_with(".part")),
        "書きかけが残らないこと"
    );

    // ── 進み具合が動くこと ──
    let after = studio.progress().expect("進み具合を引ける");
    println!("被覆: {} → {}", before.covered, after.covered);
    assert!(after.covered > before.covered, "収録済み単位が増えること");
    assert_ne!(after.next_row, before.next_row, "次の行へ進むこと");

    // ── 試唱。**縦切りの終点** ──
    let Some(oto) = take.oto else {
        println!("**発声を見つけられなかった。** 無音を録った可能性がある");
        return;
    };
    println!(
        "oto: offset={:.1} consonant={:.1} cutoff={:.1} preutter={:.1} overlap={:.1} 確信度={:?}",
        oto.offset_ms,
        oto.consonant_ms,
        oto.cutoff_ms,
        oto.preutterance_ms,
        oto.overlap_ms,
        take.confidence
    );

    // ── 試唱 ──
    //
    // **音高が渡っているかの回帰テストは、ここには置けない。**
    // 素材に有声フレームが無いと、目標 F0 は全フレーム 0 になり、
    // どの音高を頼んでも同じ波形が返る。静かな部屋では毎回そうなる。
    // 決定的な検査は `koeru-synth` の `別の音高を頼めば別の音が返る` に置いてある。
    let mut rendered = Vec::new();
    for midi in [55, 60, 67] {
        let (pcm, rate) = studio
            .render_take(take.take_id, midi, 700.0)
            .unwrap_or_else(|e| panic!("MIDI {midi} を合成できること: {e}"));
        assert!(!pcm.is_empty(), "合成結果が空でないこと");
        rendered.push((midi, pcm, rate));
    }

    let confidence = take.confidence.unwrap_or(0.0);
    if take.peak < SIGNAL_FLOOR || confidence < CONFIDENCE_FLOOR {
        println!(
            "声として録れていない（ピーク {:.4} / 確信度 {confidence:.2}）。**音高の検査は飛ばす**",
            take.peak
        );
    } else {
        for (midi, pcm, rate) in &rendered {
            let want = koeru_synth::resampler::midi_to_hz(*midi);
            let got = measure_hz(pcm, *rate);
            let cents = 1200.0 * (got / want).log2();
            println!("  MIDI {midi}: 目標 {want:.1}Hz → 実測 {got:.1}Hz（{cents:+.0} セント）");
            assert!(
                cents.abs() < 50.0,
                "**目標音高で鳴ること。** 半音（100セント）の半分より近い"
            );
        }
    }

    for (midi, _, _) in &rendered {
        studio
            .preview(take.take_id, *midi, 700.0)
            .expect("鳴らせる");
        std::thread::sleep(std::time::Duration::from_millis(750));
    }
    studio.stop_preview();

    // 後片付け。**元のライブラリは temp なので消してよい。**
    drop(studio);
    let _ = std::fs::remove_dir_all(&root);
}

/// 合成結果の平均 F0（Hz）。**有声フレームだけを見る。**
fn measure_hz(pcm: &[f32], rate: u32) -> f64 {
    let x: Vec<f64> = pcm.iter().map(|s| f64::from(*s)).collect();
    let (f0, _) = koeru_synth::world::estimate_f0(
        &x,
        rate,
        koeru_synth::world::F0Method::Harvest,
        55.0,
        1100.0,
        5.0,
    );
    let voiced: Vec<f64> = f0.into_iter().filter(|v| *v > 0.0).collect();
    if voiced.is_empty() {
        return 0.0;
    }
    voiced.iter().sum::<f64>() / voiced.len() as f64
}

/// 出力デバイスが無くても、ここまでは通ること。
#[test]
fn プロジェクトを作ると録音リストが入る() {
    let root = std::env::temp_dir().join(format!("koeru-slice-list-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let mut studio = Studio::open(root.clone()).expect("開ける");
    let id = studio.create_project("リストだけ").expect("作れる");
    studio.open_project(id).expect("開ける");

    let p = studio.progress().expect("引ける");
    assert_eq!(p.covered, 0, "まだ何も録っていない");
    assert!(p.required >= 100, "中核インベントリぶんの単位があること");
    assert!(p.next_row.is_some());

    // **一覧に出ること。**
    let listed = studio.projects().expect("挙げられる");
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0]
            .1
            .as_ref()
            .expect("manifest が読めること")
            .display_name,
        "リストだけ"
    );

    let _ = mac::enumerate_input_devices();
    let _ = std::fs::remove_dir_all(&root);
}

/// **「あと何項目で歌えるか」が録るたびに動く**（`TR-RCL-17`, `TR-RCL-19`）。
///
/// 音声デバイスを要らない。台帳の上だけで確かめる。
/// **`test-hooks` が要る。** 収録を通さずに台帳を進める入口は、既定では出さない。
#[cfg(feature = "test-hooks")]
#[test]
fn 録るほど歌える曲に近づく() {
    use koeru_core::inventory::UnitSet;
    use koeru_core::reclist::generate_single;

    let root = std::env::temp_dir().join(format!("koeru-songs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let mut studio = Studio::open(root.clone()).expect("開ける");
    let id = studio.create_project("曲の進み方").expect("作れる");
    studio.open_project(id).expect("開ける");

    // **同梱曲が初期メンバとして入っている**（TR-RCL-12）。
    let before = studio.song_status().expect("引ける");
    assert_eq!(before.len(), 1, "曲バンクを持たない。同梱は最小限");
    println!(
        "{}: あと {} 項目（必要 {} / 収録 {}）",
        before[0].title, before[0].missing_units, before[0].required, before[0].covered
    );
    assert!(before[0].missing_units > 0, "まだ何も録っていない");
    assert!(!before[0].singability.is_singable());

    let p = studio.progress().expect("引ける");
    assert_eq!(p.songs_in_bank, 1);
    assert_eq!(p.singable_songs, 0);

    // 曲に要る単位を含む行を、台帳の上だけで埋めていく。
    let list = generate_single(UnitSet::Core, 5).expect("生成できる");
    let need = &before[0];
    println!(
        "必要な単位を含む行を順に埋める（必要 {} 単位）",
        need.required
    );

    let mut filled = 0;
    let mut last = before[0].missing_units;
    for row in &list {
        studio
            .mark_recorded_for_test(&row.id)
            .expect("印を付けられる");
        filled += 1;
        let now = studio.song_status().expect("引ける");
        assert!(
            now[0].missing_units <= last,
            "録るほど減ること: {} → {}",
            last,
            now[0].missing_units
        );
        last = now[0].missing_units;
        if last == 0 {
            break;
        }
    }

    let after = studio.song_status().expect("引ける");
    println!("{} 行で「{}」が歌えるようになった", filled, after[0].title);
    assert_eq!(after[0].missing_units, 0);
    assert_eq!(
        after[0].singability,
        koeru_core::song::Singability::Complete
    );
    assert_eq!(studio.progress().expect("引ける").singable_songs, 1);

    let _ = std::fs::remove_dir_all(&root);
}
