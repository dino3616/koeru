//! 録音のファイル入出力（`.wav.part`、アトミックな確定）。
//!
//! WAV の符号化そのもの（ヘッダ、16 bit への変換、ディザ、ヘッダの解析）は
//! `koeru-formats` の [`koeru_formats::wav`] が持つ（`DEC-PLT-034`）。ここに残るのは
//! ファイルを開く側——書きかけの `.wav.part` と、パスを受ける読み書きの薄い口。
//! 既存の `koeru_audio::wav::…` の経路を通すために、符号化側の名前を再輸出している。
//! **移行中。**
//!
//! ## 書き方
//!
//! `.wav.part` へストリーミング書き込みし、終了時にサイズを確定させる（`TR-REC-28`）。
//! ヘッダの長さは書き始めの時点では分からないので、0 で置いて最後に seek して埋める。
//! fsync してからアトミックな rename で最終名にする。 そこまでが「ファイル確定」で、
//! DB へのコミットはその後（`project-storage.fsl` の `finalize_file` → `commit_take`）。

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub use koeru_formats::wav::{
    DISTRIBUTION_RATE_HZ, MASTER_RATE_HZ, Wav, WavError, WavInfo, distribution_bytes,
};

type Result<T> = std::result::Result<T, WavError>;

fn io(op: &'static str) -> impl FnOnce(std::io::Error) -> WavError {
    move |source| WavError::Io { op, source }
}

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
    #[tracing::instrument(skip(final_path), fields(rate_hz))]
    pub fn create(final_path: impl AsRef<Path>, rate_hz: u32) -> Result<Self> {
        let final_path = final_path.as_ref().to_path_buf();
        let mut part_path = final_path.clone().into_os_string();
        part_path.push(".part");
        let part_path = PathBuf::from(part_path);

        let file = File::create(&part_path).map_err(io("create"))?;
        let mut writer = BufWriter::new(file);
        // 長さはまだ分からないので 0 で置く。確定時に seek して埋める。
        koeru_formats::wav::write_master_header(&mut writer, rate_hz, 0)?;
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
    #[tracing::instrument(skip(self), fields(frames = self.frames))]
    pub fn finalize(mut self) -> Result<PathBuf> {
        self.writer.flush().map_err(io("flush"))?;
        let mut file = self.writer.into_inner().map_err(|e| WavError::Io {
            op: "into_inner",
            source: e.into_error(),
        })?;

        // 長さを埋める。
        let data_bytes = self.frames * 4;
        file.seek(SeekFrom::Start(0)).map_err(io("seek_header"))?;
        koeru_formats::wav::write_master_header(&mut file, self.rate_hz, data_bytes)?;
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

/// マスター（32 bit float / モノラル）または配布用（16 bit / モノラル）を読む。
///
/// 符号化は [`koeru_formats::wav::read`]。 ここはファイルを開くだけ。
#[tracing::instrument(skip(path))]
pub fn read(path: impl AsRef<Path>) -> Result<Wav> {
    let mut file = File::open(path.as_ref()).map_err(io("open"))?;
    koeru_formats::wav::read(&mut file)
}

/// 配布用（44100 Hz / 16 bit / モノラル）として書く（`TR-REC-01`）。
///
/// 中身は [`distribution_bytes`]。 TPDF ディザのみを適用する（`TR-REC-37`）。
#[tracing::instrument(skip(path, samples), fields(frames = samples.len()))]
pub fn write_distribution(path: impl AsRef<Path>, samples: &[f32], dither: bool) -> Result<()> {
    let file = File::create(path.as_ref()).map_err(io("create"))?;
    let mut w = BufWriter::new(file);
    koeru_formats::wav::write_distribution(&mut w, samples, dither)?;
    w.flush().map_err(io("flush"))?;
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

    /// 確定したファイルのヘッダは、書いた長さとレートを言う。
    #[test]
    fn 確定したヘッダは書いた長さを言う() {
        let path = tmp("header");
        let mut t = PartialTake::create(&path, 22_050).expect("開ける");
        t.write(&[0.1; 300]).expect("書ける");
        let final_path = t.finalize().expect("確定できる");

        let mut file = File::open(&final_path).expect("開ける");
        let info = koeru_formats::wav::inspect(&mut file).expect("ヘッダを読める");
        assert_eq!((info.rate_hz, info.samples), (22_050, 300));
        std::fs::remove_file(&final_path).ok();
    }

    #[test]
    fn 配布用はパスへ書いて読み戻せる() {
        let path = tmp("dist");
        write_distribution(&path, &[0.0, 0.5, -0.5], false).expect("書ける");
        let w = read(&path).expect("読める");
        assert_eq!(w.rate_hz, DISTRIBUTION_RATE_HZ);
        assert_eq!(w.samples.len(), 3);
        assert!((w.samples[1] - 0.5).abs() < 0.001, "0.5 が復元される");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn 無いファイルは開けない() {
        let e = read(tmp("nothing")).expect_err("開けない");
        assert!(matches!(e, WavError::Io { op: "open", .. }));
    }
}
