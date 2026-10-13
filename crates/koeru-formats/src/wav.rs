//! WAV の符号化と復号（`DEC-SYN-005`、`DEC-PLT-034`）。
//!
//! KOERU が要る形式は2つだけ。
//!
//! | | レート | 形式 | ch |
//! |---|---|---|---|
//! | マスター | 44100 Hz | 32 bit float | 1 |
//! | 配布用 | 44100 Hz | 16 bit PCM | 1 |
//!
//! 汎用の読み書きが要らないので自前で持つ。`hound` も `dr_wav` も個人のリポジトリで、
//! 束ねる相手を組織メンテのものに限るという方針（`DEC-REC-001`）に対して、
//! この規模のものに例外を作らない。
//!
//! 受け取るのはバイト列と `Read` / `Write` で、ファイルは開かない。
//! `.wav.part` への書き込みと rename（`TR-REC-28`）は `koeru-audio` が持つ。
//!
//! # ヘッダだけを読む
//!
//! [`parse_header`] と [`inspect`] は、サンプルを読まずにレート・長さ・サンプル数を返す。
//! 書き出し前の検証は、これで足りる。 全テイクを読み直すと、画面の状態を引くたびに
//! 数百 MB を読むことになる。

use std::io::{Read, Seek, SeekFrom, Write};

/// WAV の符号化と復号が失敗した理由。
#[derive(Debug, thiserror::Error)]
pub enum WavError {
    /// 読み書きが失敗した。`op` はどの操作かを示す固定の名前。
    #[error("WAV の読み書きが失敗した")]
    Io {
        op: &'static str,
        #[source]
        source: std::io::Error,
    },

    /// RIFF/WAVE として読めない。
    #[error("WAV として読めない")]
    Malformed { reason: &'static str },

    /// KOERU が扱わない形式。
    #[error("扱わない形式の WAV")]
    Unsupported {
        fmt: u16,
        bits: u16,
        channels: u16,
        rate: u32,
    },
}

impl koeru_failure::Failure for WavError {
    fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "wav.io_failed",
            Self::Malformed { .. } => "wav.malformed",
            Self::Unsupported { .. } => "wav.unsupported_format",
        }
    }

    /// 読む WAV は KOERU が書いたマスターだけなので、読めないのは壊れている。
    fn class(&self) -> koeru_failure::Class {
        match self {
            Self::Io { source, .. } => koeru_failure::io_class(source),
            Self::Malformed { .. } => koeru_failure::Class::Corrupt,
            Self::Unsupported { .. } => koeru_failure::Class::Unsupported,
        }
    }
}

type Result<T> = std::result::Result<T, WavError>;

fn io(op: &'static str) -> impl FnOnce(std::io::Error) -> WavError {
    move |source| WavError::Io { op, source }
}

/// マスターの標本化周波数（`TR-REC-02`）。
///
/// キャプチャはデバイスのネイティブレートで受け、pump が1回だけここへ変換する
/// （`DEC-REC-006`）。 そこから下流はレートを持ち回らず、この値を使う。
///
/// [`distribution_bytes`] はヘッダに 44100 と書くだけでサンプルを変換しない。
/// 44100 でないものを渡すと、44100 と名乗る別のレートの音が配られる。踏んだ。
pub const MASTER_RATE_HZ: u32 = 44_100;
/// 配布用の標本化周波数（`TR-REC-01`）。
pub const DISTRIBUTION_RATE_HZ: u32 = 44_100;

/// 形式の番号（`fmt ` チャンクの先頭）。
const FMT_PCM: u16 = 1;
const FMT_FLOAT: u16 = 3;
/// RIFF + `fmt ` + `data` のヘッダまで。 KOERU が書く WAV はこの並びだけ。
pub const HEADER_LEN: usize = 44;

/// ヘッダだけから分かること。
///
/// `fmt` / `bits` / `channels` は、扱わない形式でも読めたとおりに持つ。
/// 扱う形式かどうかは [`WavInfo::ensure_supported`] が見る。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo {
    /// 形式の番号。 1 が PCM、3 が float。
    pub fmt: u16,
    pub channels: u16,
    pub rate_hz: u32,
    /// 1サンプルあたりのビット数。
    pub bits: u16,
    /// `data` チャンクが宣言している長さ（バイト）。
    pub data_bytes: u32,
    /// サンプル数。 `data_bytes` を1サンプルのバイト数で割った商（モノラルなのでフレーム数と同じ）。
    pub samples: u64,
}

impl WavInfo {
    /// KOERU が扱う形（モノラルの 32 bit float か 16 bit PCM）か。
    ///
    /// # Errors
    ///
    /// ステレオや 24 bit など、扱わない形式。
    pub const fn ensure_supported(&self) -> Result<()> {
        if self.channels == 1 && matches!((self.fmt, self.bits), (FMT_FLOAT, 32) | (FMT_PCM, 16)) {
            Ok(())
        } else {
            Err(WavError::Unsupported {
                fmt: self.fmt,
                bits: self.bits,
                channels: self.channels,
                rate: self.rate_hz,
            })
        }
    }

    /// 配布用（44100 Hz / 16 bit PCM / モノラル）か（`TR-PKG-20`）。
    #[must_use]
    pub const fn is_distribution(&self) -> bool {
        self.fmt == FMT_PCM
            && self.channels == 1
            && self.rate_hz == DISTRIBUTION_RATE_HZ
            && self.bits == 16
    }
}

/// 先頭 [`HEADER_LEN`] バイトを読む。
///
/// サンプルには触らない。 `bytes` が長くてもヘッダの外は見ない。
///
/// # Errors
///
/// RIFF/WAVE でない、`fmt ` / `data` が決まった位置に無い、短すぎる。
/// 扱わない形式かどうかはここでは見ない（[`WavInfo::ensure_supported`]）。
pub fn parse_header(bytes: &[u8]) -> Result<WavInfo> {
    let Some(header) = bytes.first_chunk::<HEADER_LEN>() else {
        return Err(WavError::Malformed {
            reason: "ヘッダより短い",
        });
    };
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Err(WavError::Malformed {
            reason: "RIFF/WAVE ではない",
        });
    }
    if &header[12..16] != b"fmt " {
        return Err(WavError::Malformed {
            reason: "fmt チャンクが先頭に無い",
        });
    }
    let fmt = u16::from_le_bytes([header[20], header[21]]);
    let channels = u16::from_le_bytes([header[22], header[23]]);
    let rate_hz = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
    let bits = u16::from_le_bytes([header[34], header[35]]);
    if &header[36..40] != b"data" {
        return Err(WavError::Malformed {
            reason: "data チャンクが fmt の直後に無い",
        });
    }
    let data_bytes = u32::from_le_bytes([header[40], header[41], header[42], header[43]]);
    let bytes_per_sample = u32::from(bits / 8).max(1);
    Ok(WavInfo {
        fmt,
        channels,
        rate_hz,
        bits,
        data_bytes,
        samples: u64::from(data_bytes / bytes_per_sample),
    })
}

/// ヘッダを読み、`data` がヘッダの宣言どおりに収まっているかまで確かめる。
///
/// サンプルは読まない（先頭 [`HEADER_LEN`] バイトと、全体の長さを知るための seek だけ）。
/// 戻ったあとの読み位置は決まっていない。
///
/// 長さを見るのは、書きかけや切れたファイルをヘッダだけで通さないため。
/// [`read`] は足りなければ失敗するので、ここも同じ側に倒す。
///
/// # Errors
///
/// 読めない、ヘッダが壊れている、扱わない形式、`data` が宣言より短い。
pub fn inspect<R: Read + Seek>(r: &mut R) -> Result<WavInfo> {
    let len = r.seek(SeekFrom::End(0)).map_err(io("seek_end"))?;
    r.seek(SeekFrom::Start(0)).map_err(io("seek_start"))?;
    let mut header = [0_u8; HEADER_LEN];
    r.read_exact(&mut header).map_err(io("read_header"))?;
    let info = parse_header(&header)?;
    info.ensure_supported()?;
    if len < HEADER_LEN as u64 + u64::from(info.data_bytes) {
        return Err(WavError::Malformed {
            reason: "data がヘッダの宣言より短い",
        });
    }
    Ok(info)
}

/// 読み込んだ WAV。
#[derive(Debug, Clone)]
pub struct Wav {
    pub samples: Vec<f32>,
    pub rate_hz: u32,
}

/// マスター（32 bit float / モノラル）または配布用（16 bit / モノラル）を読む。
///
/// # Errors
///
/// 読めない、ヘッダが壊れている、扱わない形式、`data` が宣言より短い。
pub fn read<R: Read>(r: &mut R) -> Result<Wav> {
    let mut header = [0_u8; HEADER_LEN];
    r.read_exact(&mut header).map_err(io("read_header"))?;
    let info = parse_header(&header)?;
    // KOERU が扱うのは2形式だけ（`DEC-SYN-005`）。
    info.ensure_supported()?;

    // 長さはヘッダの申告で、信じて先に確保しない。 壊れたヘッダが数 GB を要求できてしまう。
    let want = u64::from(info.data_bytes);
    let mut data = Vec::new();
    r.take(want)
        .read_to_end(&mut data)
        .map_err(io("read_data"))?;
    if (data.len() as u64) < want {
        return Err(WavError::Io {
            op: "read_data",
            source: std::io::ErrorKind::UnexpectedEof.into(),
        });
    }
    let samples = match (info.fmt, info.bits) {
        (FMT_FLOAT, 32) => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect(),
        _ => data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| f32::from(i16::from_le_bytes(*c)) / 32768.0)
            .collect(),
    };
    Ok(Wav {
        samples,
        rate_hz: info.rate_hz,
    })
}

/// バイト列から読む。 [`read`] と同じ。
///
/// # Errors
///
/// [`read`] と同じ。
pub fn decode(bytes: &[u8]) -> Result<Wav> {
    read(&mut &*bytes)
}

/// 配布用 WAV をバイト列で組み立てる（`TR-REC-01`）。
///
/// 配布パッケージは ZIP を組み立てながら作るので、WAV を一度ファイルへ
/// 書いてから読み直さない。
///
/// **レートは常に [`DISTRIBUTION_RATE_HZ`]。** 呼び出し側のレートを
/// 引き取らないのは、マスターが 44100 でないまま配ると「44100 と名乗る
/// 別のレートの音」になるため（`TR-REC-02`, `DEC-REC-006`）。
///
/// TPDF ディザのみを適用する（`TR-REC-37`）。音色を変える処理は行わない。
/// 乱数は固定の種から始めるので、同じ入力からは同じバイト列が出る（`TR-EDT-41`）。
#[must_use]
pub fn distribution_bytes(samples: &[f32], dither: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + samples.len() * 2);
    // Vec への書き込みは失敗しない。
    let _ = write_header(
        &mut out,
        DISTRIBUTION_RATE_HZ,
        FMT_PCM,
        16,
        (samples.len() * 2) as u64,
    );

    // TPDF ディザ（振幅 1 LSB）。2つの一様乱数の和で三角分布にする。
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut tpdf = move || {
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // [0, 1) の一様乱数
            (state >> 11) as f32 / (1_u64 << 53) as f32
        };
        next() - next()
    };

    for s in samples {
        let scaled = s * 32767.0;
        let with_dither = if dither { scaled + tpdf() } else { scaled };
        #[allow(clippy::cast_possible_truncation)]
        let v = with_dither.round().clamp(-32768.0, 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// 配布用 WAV を書く。 中身は [`distribution_bytes`]。
///
/// # Errors
///
/// 書き込みが失敗した。
pub fn write_distribution<W: Write>(w: &mut W, samples: &[f32], dither: bool) -> Result<()> {
    w.write_all(&distribution_bytes(samples, dither))
        .map_err(io("write"))
}

/// マスター（32 bit float / モノラル）のヘッダを書く。
///
/// `data_bytes` は書き始めの時点では分からないので、0 で置いて最後に埋め直す
/// （`TR-REC-28`）。
///
/// # Errors
///
/// 書き込みが失敗した。
pub fn write_master_header<W: Write>(w: &mut W, rate_hz: u32, data_bytes: u64) -> Result<()> {
    write_header(w, rate_hz, FMT_FLOAT, 32, data_bytes).map_err(io("write_header"))
}

fn write_header<W: Write>(
    w: &mut W,
    rate_hz: u32,
    fmt: u16,
    bits: u16,
    data_bytes: u64,
) -> std::io::Result<()> {
    let block_align = bits / 8; // モノラル
    let byte_rate = rate_hz * u32::from(block_align);
    #[allow(clippy::cast_possible_truncation)]
    let data_len = data_bytes as u32;

    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?;
    w.write_all(&16_u32.to_le_bytes())?;
    w.write_all(&fmt.to_le_bytes())?;
    w.write_all(&1_u16.to_le_bytes())?; // モノラル
    w.write_all(&rate_hz.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&bits.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// マスター（32 bit float）を、`PartialTake` と同じ手順でバイト列にする。
    fn master_bytes(rate_hz: u32, samples: &[f32]) -> Vec<u8> {
        let mut out = Vec::new();
        write_master_header(&mut out, rate_hz, samples.len() as u64 * 4).expect("書ける");
        for s in samples {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }

    /// FNV-1a。 試験の中だけで、バイト列の同一性を1つの数で言うために使う。
    fn fnv1a(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// 決まった入力。 疑似乱数と正弦波を足して、ディザの効きが見えるようにしてある。
    fn input() -> Vec<f32> {
        let mut s = 7_u32;
        (0..2000)
            .map(|i| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (s >> 16) as f32 / 65536.0 - 0.5;
                (i as f32 * 0.05).sin() * 0.4 + noise * 0.05
            })
            .collect()
    }

    #[test]
    fn マスターを書いて読み戻せる() {
        let bytes = master_bytes(MASTER_RATE_HZ, &[0.0, 0.5, -0.5, 1.0]);
        let w = decode(&bytes).expect("読める");
        assert_eq!(w.rate_hz, MASTER_RATE_HZ);
        assert_eq!(w.samples, vec![0.0, 0.5, -0.5, 1.0]);
    }

    #[test]
    fn 配布用は十六ビットで書ける() {
        let bytes = distribution_bytes(&[0.0, 0.5, -0.5], false);
        let w = decode(&bytes).expect("読める");
        assert_eq!(w.rate_hz, DISTRIBUTION_RATE_HZ);
        assert_eq!(w.samples.len(), 3);
        assert!((w.samples[1] - 0.5).abs() < 0.001, "0.5 が復元される");
    }

    /// ディザは値を1 LSB ぶんしか動かさない（`TR-REC-37`）。
    #[test]
    fn ディザは最下位ビット一つぶんに収まる() {
        let src: Vec<f32> = (0..1000).map(|i| (i as f32 / 1000.0) - 0.5).collect();
        let w = decode(&distribution_bytes(&src, true)).expect("読める");
        for (a, b) in src.iter().zip(&w.samples) {
            let diff = (a - b).abs();
            assert!(diff < 2.0 / 32768.0, "差が 1 LSB 程度に収まる: {diff}");
        }
    }

    /// 同じ入力からは同じバイト列が出る（`TR-EDT-41`）。 種が固定でなくなると、
    /// 同じ音源を2度書き出した ZIP が一致しなくなる。
    #[test]
    fn 配布用のバイト列は何度作っても同じ() {
        let a = distribution_bytes(&input(), true);
        let b = distribution_bytes(&input(), true);
        assert_eq!(a, b);
        assert_ne!(
            a,
            distribution_bytes(&input(), false),
            "ディザは実際に値を動かす"
        );
    }

    /// 配布 WAV のバイト列を固定する（`TR-PKG-20`、`TR-REC-37`）。 符号化を移しても
    /// 配られる音が1ビットも変わらないことを、数1つで見る。
    ///
    /// 変えるときは、ディザの種か量子化を意図して変えたということ。 配布済みの音源と
    /// 同じ入力から別のバイト列が出るので、黙って更新しない。
    #[test]
    fn 配布用のバイト列を固定する() {
        let dithered = distribution_bytes(&input(), true);
        let plain = distribution_bytes(&input(), false);
        assert_eq!(dithered.len(), 44 + 2000 * 2);
        assert_eq!(
            (fnv1a(&dithered), fnv1a(&plain)),
            (PINNED_DITHERED, PINNED_PLAIN),
            "{:#x} {:#x}",
            fnv1a(&dithered),
            fnv1a(&plain)
        );
    }

    const PINNED_DITHERED: u64 = 0x5898_e079_c0fa_cd8c;
    const PINNED_PLAIN: u64 = 0x4134_8ec0_8091_0627;

    #[test]
    fn ヘッダだけでレートと長さが分かる() {
        let bytes = master_bytes(22_050, &[0.1; 300]);
        let info = parse_header(&bytes).expect("読める");
        assert_eq!(info.rate_hz, 22_050);
        assert_eq!((info.fmt, info.channels, info.bits), (FMT_FLOAT, 1, 32));
        assert_eq!(info.data_bytes, 1200);
        assert_eq!(info.samples, 300);
        // ヘッダの外は見ない。 先頭 44 バイトだけでも同じ答えになる。
        assert_eq!(parse_header(&bytes[..HEADER_LEN]).expect("読める"), info);
    }

    /// ヘッダだけの答えが、全部読んだ答えと食い違わない。
    #[test]
    fn ヘッダだけの答えは全部読んだ答えと一致する() {
        let cases = [
            master_bytes(MASTER_RATE_HZ, &input()),
            master_bytes(22_050, &[0.25; 17]),
            distribution_bytes(&input(), true),
            distribution_bytes(&[], false),
        ];
        for bytes in cases {
            let full = decode(&bytes).expect("全部読める");
            let info = inspect(&mut Cursor::new(&bytes)).expect("ヘッダを読める");
            assert_eq!(info.rate_hz, full.rate_hz);
            assert_eq!(info.samples, full.samples.len() as u64);
        }
    }

    #[test]
    fn 短いヘッダは弾く() {
        assert!(matches!(
            parse_header(&[0_u8; 20]),
            Err(WavError::Malformed { .. })
        ));
    }

    #[test]
    fn riff_でなければ弾く() {
        let mut bytes = master_bytes(MASTER_RATE_HZ, &[0.0; 4]);
        bytes[0..4].copy_from_slice(b"RIFX");
        assert!(matches!(
            parse_header(&bytes),
            Err(WavError::Malformed { .. })
        ));
    }

    /// 書きかけや切れたファイルを、ヘッダだけで通さない。
    #[test]
    fn データが宣言より短ければヘッダだけでも弾く() {
        let mut bytes = master_bytes(MASTER_RATE_HZ, &[0.1; 100]);
        bytes.truncate(bytes.len() - 4);
        assert!(matches!(
            inspect(&mut Cursor::new(&bytes)),
            Err(WavError::Malformed { .. })
        ));
        assert!(matches!(decode(&bytes), Err(WavError::Io { .. })));
    }

    /// **扱わない形式は弾く。** ステレオや 24 bit を黙って受けない。
    #[test]
    fn 扱わない形式は弾く() {
        let mut stereo = Vec::new();
        stereo.extend_from_slice(b"RIFF");
        stereo.extend_from_slice(&36_u32.to_le_bytes());
        stereo.extend_from_slice(b"WAVEfmt ");
        stereo.extend_from_slice(&16_u32.to_le_bytes());
        stereo.extend_from_slice(&FMT_PCM.to_le_bytes());
        stereo.extend_from_slice(&2_u16.to_le_bytes()); // 2ch
        stereo.extend_from_slice(&44100_u32.to_le_bytes());
        stereo.extend_from_slice(&176_400_u32.to_le_bytes());
        stereo.extend_from_slice(&4_u16.to_le_bytes());
        stereo.extend_from_slice(&16_u16.to_le_bytes());
        stereo.extend_from_slice(b"data");
        stereo.extend_from_slice(&0_u32.to_le_bytes());

        assert!(matches!(
            decode(&stereo),
            Err(WavError::Unsupported { channels: 2, .. })
        ));
        assert!(matches!(
            inspect(&mut Cursor::new(&stereo)),
            Err(WavError::Unsupported { channels: 2, .. })
        ));
        // 読めてはいる。 扱うかどうかは呼び出し側が見る。
        assert!(
            parse_header(&stereo)
                .expect("読める")
                .ensure_supported()
                .is_err()
        );
    }

    #[test]
    fn 配布用の形かどうかを見分ける() {
        let dist = parse_header(&distribution_bytes(&[0.0; 4], false)).expect("読める");
        assert!(dist.is_distribution());
        let master = parse_header(&master_bytes(MASTER_RATE_HZ, &[0.0; 4])).expect("読める");
        assert!(!master.is_distribution(), "32 bit float は配布の形ではない");
        let slow = parse_header(&master_bytes(22_050, &[0.0; 4])).expect("読める");
        assert!(!slow.is_distribution());
    }
}
