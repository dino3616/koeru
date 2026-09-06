//! WAV の入出力。
//!
//! KOERU が要る形式は2つだけ（`DEC-SYN-005`）。
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
//! ## 書き方
//!
//! `.wav.part` へストリーミング書き込みし、終了時にサイズを確定させる（`TR-REC-28`）。
//! ヘッダの長さは書き始めの時点では分からないので、0 で置いて最後に seek して埋める。
//! fsync してからアトミックな rename で最終名にする。 そこまでが「ファイル確定」で、
//! DB へのコミットはその後（`project-storage.fsl` の `finalize_file` → `commit_take`）。

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// WAV の読み書きが失敗した理由。
#[derive(Debug, thiserror::Error)]
pub enum WavError {
    /// ファイル操作が失敗した。
    #[error("ファイル操作が失敗した（{op}）")]
    Io {
        op: &'static str,
        #[source]
        source: std::io::Error,
    },

    /// RIFF/WAVE として読めない。
    #[error("WAV として読めない（{reason}）")]
    Malformed { reason: &'static str },

    /// KOERU が扱わない形式。
    #[error("扱わない形式（{fmt} 形式、{bits} bit、{channels}ch、{rate}Hz）")]
    Unsupported {
        fmt: u16,
        bits: u16,
        channels: u16,
        rate: u32,
    },
}

impl WavError {
    /// 送信層へ載せてよい固定文字列。`Display` を送らない（パスが入りうる）。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Io { .. } => "wav.io_failed",
            Self::Malformed { .. } => "wav.malformed",
            Self::Unsupported { .. } => "wav.unsupported_format",
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
/// [`write_distribution`] はヘッダに 44100 と書くだけでサンプルを変換しない。
/// 44100 でないものを渡すと、44100 と名乗る別のレートの音が配られる。踏んだ。
pub const MASTER_RATE_HZ: u32 = 44_100;
/// 配布用の標本化周波数（`TR-REC-01`）。
pub const DISTRIBUTION_RATE_HZ: u32 = 44_100;

const FMT_PCM: u16 = 1;
const FMT_FLOAT: u16 = 3;
const HEADER_LEN: u64 = 44;

/// 書きかけのテイク。
///
/// 落とすまで `.wav.part` のまま。 [`PartialTake::finalize`] を呼ぶと
/// サイズを確定させ、fsync してからアトミックに rename する（`TR-REC-28`）。
#[derive(Debug)]
pub struct PartialTake {
    writer: BufWriter<File>,
    part_path: PathBuf,
    final_path: PathBuf,
    frames: u64,
    rate_hz: u32,
}

impl PartialTake {
    /// `final_path` に対応する `.wav.part` を開く（`TR-REC-28`）。
    ///
    /// マスターの形式（32 bit float / モノラル）で書き始める。
    #[tracing::instrument(skip(final_path), fields(rate_hz), err)]
    pub fn create(final_path: impl AsRef<Path>, rate_hz: u32) -> Result<Self> {
        let final_path = final_path.as_ref().to_path_buf();
        let mut part_path = final_path.clone().into_os_string();
        part_path.push(".part");
        let part_path = PathBuf::from(part_path);

        let file = File::create(&part_path).map_err(io("create"))?;
        let mut writer = BufWriter::new(file);
        // 長さはまだ分からないので 0 で置く。確定時に seek して埋める。
        write_header(&mut writer, rate_hz, FMT_FLOAT, 32, 0).map_err(io("write_header"))?;
        Ok(Self {
            writer,
            part_path,
            final_path,
            frames: 0,
            rate_hz,
        })
    }

    /// サンプルを書き足す。モノラルの 32 bit float。
    pub fn write(&mut self, samples: &[f32]) -> Result<()> {
        for s in samples {
            self.writer
                .write_all(&s.to_le_bytes())
                .map_err(io("write_samples"))?;
        }
        self.frames += samples.len() as u64;
        Ok(())
    }

    /// いままでに書いたフレーム数。
    #[must_use]
    pub const fn frames(&self) -> u64 {
        self.frames
    }

    /// ファイルを確定させる（`TR-REC-28` / `project-storage.fsl` の `finalize_file`）。
    ///
    /// 順序が契約そのもの。サイズを確定 → fsync → アトミックな rename。
    /// ここまでが済んでから DB へコミットする。逆にすると、ファイルの無い行が DB に残る。
    #[tracing::instrument(skip(self), fields(frames = self.frames), err)]
    pub fn finalize(mut self) -> Result<PathBuf> {
        self.writer.flush().map_err(io("flush"))?;
        let mut file = self.writer.into_inner().map_err(|e| WavError::Io {
            op: "into_inner",
            source: e.into_error(),
        })?;

        // 長さを埋める。
        let data_bytes = self.frames * 4;
        file.seek(SeekFrom::Start(0)).map_err(io("seek_header"))?;
        write_header(&mut file, self.rate_hz, FMT_FLOAT, 32, data_bytes)
            .map_err(io("rewrite_header"))?;
        file.flush().map_err(io("flush_header"))?;

        // fsync してから rename する。 逆だと、rename は済んでいるのに
        // 中身がディスクに無い状態が電源喪失で残りうる。
        file.sync_all().map_err(io("fsync"))?;
        drop(file);

        std::fs::rename(&self.part_path, &self.final_path).map_err(io("rename"))?;
        tracing::debug!(frames = self.frames, "テイクのファイルを確定した");
        Ok(self.final_path.clone())
    }

    /// 書きかけを捨てる（`discard_invalid_take`）。
    ///
    /// `.part` を消すだけ。確定済みのファイルには触らない。
    pub fn discard(self) -> Result<()> {
        let path = self.part_path.clone();
        drop(self.writer);
        std::fs::remove_file(&path).map_err(io("remove_part"))?;
        Ok(())
    }
}

/// 読み込んだ WAV。
#[derive(Debug, Clone)]
pub struct Wav {
    pub samples: Vec<f32>,
    pub rate_hz: u32,
}

/// マスター（32 bit float / モノラル）または配布用（16 bit / モノラル）を読む。
#[tracing::instrument(skip(path), err)]
pub fn read(path: impl AsRef<Path>) -> Result<Wav> {
    let mut file = File::open(path.as_ref()).map_err(io("open"))?;
    let mut header = [0_u8; HEADER_LEN as usize];
    file.read_exact(&mut header).map_err(io("read_header"))?;

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
    let data_bytes = u32::from_le_bytes([header[40], header[41], header[42], header[43]]) as usize;

    // KOERU が扱うのは2形式だけ（`DEC-SYN-005`）。
    if channels != 1 || !matches!((fmt, bits), (FMT_FLOAT, 32) | (FMT_PCM, 16)) {
        return Err(WavError::Unsupported {
            fmt,
            bits,
            channels,
            rate: rate_hz,
        });
    }

    let mut data = vec![0_u8; data_bytes];
    file.read_exact(&mut data).map_err(io("read_data"))?;
    let samples = match (fmt, bits) {
        (FMT_FLOAT, 32) => data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        _ => data
            .chunks_exact(2)
            .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0)
            .collect(),
    };
    Ok(Wav { samples, rate_hz })
}

/// 配布用（44100 Hz / 16 bit / モノラル）として書く（`TR-REC-01`）。
///
/// TPDF ディザのみを適用する（`TR-REC-37`）。音色を変える処理は行わない。
#[tracing::instrument(skip(path, samples), fields(frames = samples.len()), err)]
pub fn write_distribution(path: impl AsRef<Path>, samples: &[f32], dither: bool) -> Result<()> {
    let file = File::create(path.as_ref()).map_err(io("create"))?;
    let mut w = BufWriter::new(file);
    write_header(
        &mut w,
        DISTRIBUTION_RATE_HZ,
        FMT_PCM,
        16,
        (samples.len() * 2) as u64,
    )
    .map_err(io("write_header"))?;

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
        w.write_all(&v.to_le_bytes()).map_err(io("write_samples"))?;
    }
    w.flush().map_err(io("flush"))?;
    Ok(())
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

    fn tmp(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("koeru-wav-test-{name}-{}.wav", std::process::id()));
        p
    }

    #[test]
    fn 書いて読み戻せる() {
        let path = tmp("roundtrip");
        let mut t = PartialTake::create(&path, MASTER_RATE_HZ).expect("開ける");
        t.write(&[0.0, 0.5, -0.5, 1.0]).expect("書ける");
        let final_path = t.finalize().expect("確定できる");

        let w = read(&final_path).expect("読める");
        assert_eq!(w.rate_hz, MASTER_RATE_HZ);
        assert_eq!(w.samples, vec![0.0, 0.5, -0.5, 1.0]);
        std::fs::remove_file(&final_path).ok();
    }

    /// 確定するまで最終名のファイルは存在しない（`TR-REC-28`）。
    #[test]
    fn 確定するまで最終名は現れない() {
        let path = tmp("part_only");
        let mut t = PartialTake::create(&path, MASTER_RATE_HZ).expect("開ける");
        t.write(&[0.1; 100]).expect("書ける");
        assert!(!path.exists(), "確定前に最終名は無い");
        let part = path.with_extension("wav.part");
        assert!(part.exists(), ".part がある");
        let final_path = t.finalize().expect("確定できる");
        assert!(final_path.exists(), "確定後に最終名が現れる");
        std::fs::remove_file(&final_path).ok();
    }

    /// 書きかけを捨てても、最終名のファイルはできない。
    #[test]
    fn 捨てると何も残らない() {
        let path = tmp("discard");
        let mut t = PartialTake::create(&path, MASTER_RATE_HZ).expect("開ける");
        t.write(&[0.1; 10]).expect("書ける");
        t.discard().expect("捨てられる");
        assert!(!path.exists());
        assert!(!path.with_extension("wav.part").exists());
    }

    #[test]
    fn 配布用は十六ビットで書ける() {
        let path = tmp("dist");
        write_distribution(&path, &[0.0, 0.5, -0.5], false).expect("書ける");
        let w = read(&path).expect("読める");
        assert_eq!(w.rate_hz, DISTRIBUTION_RATE_HZ);
        assert_eq!(w.samples.len(), 3);
        assert!((w.samples[1] - 0.5).abs() < 0.001, "0.5 が復元される");
        std::fs::remove_file(&path).ok();
    }

    /// ディザは値を1 LSB ぶんしか動かさない（`TR-REC-37`）。
    #[test]
    fn ディザは最下位ビット一つぶんに収まる() {
        let path = tmp("dither");
        let src: Vec<f32> = (0..1000).map(|i| (i as f32 / 1000.0) - 0.5).collect();
        write_distribution(&path, &src, true).expect("書ける");
        let w = read(&path).expect("読める");
        for (a, b) in src.iter().zip(&w.samples) {
            let diff = (a - b).abs();
            assert!(diff < 2.0 / 32768.0, "差が 1 LSB 程度に収まる: {diff}");
        }
        std::fs::remove_file(&path).ok();
    }

    /// **扱わない形式は弾く。** ステレオや 24 bit を黙って受けない。
    #[test]
    fn 扱わない形式は弾く() {
        let path = tmp("stereo");
        let file = File::create(&path).expect("作れる");
        let mut w = BufWriter::new(file);
        // ステレオのヘッダを手で書く
        w.write_all(b"RIFF").unwrap();
        w.write_all(&36_u32.to_le_bytes()).unwrap();
        w.write_all(b"WAVEfmt ").unwrap();
        w.write_all(&16_u32.to_le_bytes()).unwrap();
        w.write_all(&FMT_PCM.to_le_bytes()).unwrap();
        w.write_all(&2_u16.to_le_bytes()).unwrap(); // 2ch
        w.write_all(&44100_u32.to_le_bytes()).unwrap();
        w.write_all(&176_400_u32.to_le_bytes()).unwrap();
        w.write_all(&4_u16.to_le_bytes()).unwrap();
        w.write_all(&16_u16.to_le_bytes()).unwrap();
        w.write_all(b"data").unwrap();
        w.write_all(&0_u32.to_le_bytes()).unwrap();
        w.flush().unwrap();
        drop(w);

        assert!(matches!(
            read(&path),
            Err(WavError::Unsupported { channels: 2, .. })
        ));
        std::fs::remove_file(&path).ok();
    }
}
