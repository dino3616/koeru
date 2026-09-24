//! 音源アイコン（`TR-PKG-07`, `DEC-PKG-012`）。
//!
//! 本人が選んだ PNG / JPEG を 100×100 の BMP へ変換する。
//! BMP に固定するのは `TR-PKG-07` の判断で、classic UTAU が PNG を
//! 表示できるかを一次仕様として確認できていないため。

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, ImageReader, Limits, RgbImage, imageops::FilterType};

/// 音源アイコンの一辺（`TR-PKG-07`）。
pub const SIZE: u32 = 100;

/// 受け付ける絵の一辺の上限（px）。
///
/// **圧縮後の大きさでは足りない。** 8 MiB に収まる PNG が、展開すると
/// 数百 MB の画素を要求することがある（`load_from_memory` は縮める前に
/// それを全部確保する）。宣言された寸法の側で止める。
const MAX_SIDE: u32 = 8_000;

/// 展開に使ってよいバイト数の上限。
///
/// 8000 × 8000 の RGBA でおよそ 256 MB。 立ち絵としてはこれで十分広い。
const MAX_ALLOC: u64 = 256 * 1024 * 1024;

/// アイコンを作れなかった理由。
#[derive(Debug, thiserror::Error)]
pub enum IconError {
    /// 画像として読めない。対応しているのは PNG と JPEG（`DEC-PKG-012`）。
    #[error("画像として読めない")]
    Undecodable {
        #[source]
        source: image::ImageError,
    },

    /// 展開すると大きすぎる。圧縮後の大きさでは止められない。
    #[error("絵が大きすぎる")]
    TooLarge { width: u32, height: u32 },

    /// BMP として書けない。
    #[error("BMP として書けない")]
    Unencodable {
        #[source]
        source: image::ImageError,
    },
}

/// 画像そのものは code にも文言にも入れない。画像は本人の創作物。
impl koeru_failure::Failure for IconError {
    fn code(&self) -> &'static str {
        match self {
            Self::Undecodable { .. } => "icon.undecodable",
            Self::TooLarge { .. } => "icon.too_large",
            Self::Unencodable { .. } => "icon.unencodable",
        }
    }

    fn class(&self) -> koeru_failure::Class {
        match self {
            Self::Undecodable { .. } | Self::TooLarge { .. } => koeru_failure::Class::InvalidInput,
            // 読めた画像を縮めて書くのは KOERU の側。
            Self::Unencodable { .. } => koeru_failure::Class::Internal,
        }
    }
}

type Result<T> = std::result::Result<T, IconError>;

/// 元画像から 100×100 の BMP を作る（`TR-PKG-07`）。
///
/// 縦横比が違う画像は中央で切り出す。 余白を足して収めると、
/// UTAU の一覧で額縁だけが並ぶ。
///
/// # Errors
///
/// PNG / JPEG として読めない、または BMP として書けない。
#[tracing::instrument(skip(source), fields(bytes = source.len()))]
pub fn to_bmp(source: &[u8]) -> Result<Vec<u8>> {
    let img = decode(source)?;
    let square = img.resize_to_fill(SIZE, SIZE, FilterType::Lanczos3);

    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(flatten(&square))
        .write_to(&mut out, ImageFormat::Bmp)
        .map_err(|source| IconError::Unencodable { source })?;
    Ok(out.into_inner())
}

/// PNG へ揃えた立ち絵と、その高さ（`TR-PKG-07`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Portrait {
    /// PNG のバイト列。
    pub png: Vec<u8>,
    /// 絵の高さ（px）。`character.yaml` の `portrait_height` に出る。
    pub height: u32,
}

/// 立ち絵を PNG に揃え、高さを測る（`TR-PKG-07`）。
///
/// 受け取るのは PNG と JPEG（`DEC-PKG-012`）で、配布物に入る名前は
/// `portrait.png` に固定してある。**そのまま複製すると、JPEG が PNG を
/// 名乗って入る。** 中身と拡張子が食い違ったものは、読み手によって
/// 開けたり開けなかったりする。
///
/// 透過は畳まない。 立ち絵は背景に重ねるもので、`portrait_opacity` と
/// 併せて使う（アイコンとは要求が違う）。
///
/// # Errors
///
/// PNG / JPEG として読めない、または PNG として書けない。
#[tracing::instrument(skip(source), fields(bytes = source.len()))]
pub fn to_portrait(source: &[u8]) -> Result<Portrait> {
    let img = decode(source)?;
    let height = img.height();
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png)
        .map_err(|source| IconError::Unencodable { source })?;
    Ok(Portrait {
        png: out.into_inner(),
        height,
    })
}

/// 寸法を確かめてから展開する。
///
/// **展開の前に止める。** `load_from_memory` は画素を全部確保してから
/// 返すので、そこまで行くと手遅れ。宣言された寸法を読み、上限を超えていたら
/// 展開しない。`Limits` も併せて渡す——宣言が嘘でも確保で止まる。
fn decode(source: &[u8]) -> Result<DynamicImage> {
    let open = |bytes: &'_ [u8]| {
        ImageReader::new(std::io::Cursor::new(bytes.to_vec()))
            .with_guessed_format()
            .map_err(|e| IconError::Undecodable {
                source: image::ImageError::IoError(e),
            })
    };
    let (width, height) = open(source)?
        .into_dimensions()
        .map_err(|source| IconError::Undecodable { source })?;
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(IconError::TooLarge { width, height });
    }

    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    let mut reader = open(source)?;
    reader.limits(limits);
    reader
        .decode()
        .map_err(|source| IconError::Undecodable { source })
}

/// 透過を白へ畳む。
///
/// 24 bit の BMP にアルファの置き場が無い。 畳まずに捨てると、
/// 透過部分が黒く出る実装がある——UTAU 側の描画は決まっていないので、
/// ここで確定させる。
fn flatten(img: &DynamicImage) -> RgbImage {
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (x, y, px) in rgba.enumerate_pixels() {
        let a = f32::from(px[3]) / 255.0;
        let over = |c: u8| {
            let blended = f32::from(c).mul_add(a, 255.0 * (1.0 - a));
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "blended は 0..=255 に収まる"
            )]
            let v = blended.round().clamp(0.0, 255.0) as u8;
            v
        };
        out.put_pixel(x, y, image::Rgb([over(px[0]), over(px[1]), over(px[2])]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use koeru_failure::Failure;

    /// 試験用の PNG を作る。
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut img = image::RgbaImage::new(width, height);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = image::Rgba([(x % 256) as u8, (y % 256) as u8, 0, 255]);
        }
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut out, ImageFormat::Png)
            .expect("PNG を書けること");
        out.into_inner()
    }

    /// PNG のチャンクが使う CRC-32（IEEE）。
    ///
    /// 試験のためだけに要る。 `image` は計算した値を外へ出さないので、
    /// 寸法を書き換えたヘッダを作るには自分で付け直すしかない。
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFF_u32;
        for b in bytes {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                let mask = 0_u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    /// BMP のヘッダから幅と高さを読む。
    fn dimensions(bmp: &[u8]) -> (i32, i32) {
        let w = i32::from_le_bytes([bmp[18], bmp[19], bmp[20], bmp[21]]);
        let h = i32::from_le_bytes([bmp[22], bmp[23], bmp[24], bmp[25]]);
        (w, h.abs())
    }

    #[test]
    fn 正方形の_bmp_になる() {
        let bmp = to_bmp(&png(300, 300)).expect("変換できること");
        assert_eq!(&bmp[0..2], b"BM");
        assert_eq!(dimensions(&bmp), (100, 100));
    }

    /// 縦長でも横長でも 100×100。余白を足さない。
    #[test]
    fn 縦横比が違っても_100_角に収まる() {
        for (w, h) in [(400, 100), (100, 400)] {
            let bmp = to_bmp(&png(w, h)).expect("変換できること");
            assert_eq!(dimensions(&bmp), (100, 100), "{w}x{h}");
        }
    }

    /// `TR-PKG-07`。立ち絵は PNG に揃える。JPEG が PNG を名乗らないように。
    #[test]
    fn 立ち絵は_png_になる() {
        let src = png(120, 300);
        let out = to_portrait(&src).expect("変換できること");
        assert_eq!(&out.png[1..4], b"PNG");
        // 高さは絵から測る。画面に欄が無いので、ここで入らないと 0 のまま出る。
        assert_eq!(out.height, 300);
    }

    /// 寸法は変えない。切り出すのはアイコンだけ。
    #[test]
    fn 立ち絵の寸法は変えない() {
        let out = to_portrait(&png(120, 300)).expect("変換できること");
        let decoded = image::load_from_memory(&out.png).expect("読めること");
        assert_eq!((decoded.width(), decoded.height()), (120, 300));
    }

    /// 展開すると大きすぎる絵は、展開の前に止める。
    ///
    /// **圧縮後の大きさでは止められない。** 宣言だけ巨大な PNG は小さく作れる。
    #[test]
    fn 大きすぎる絵は展開の前に止まる() {
        // 幅と高さだけを書き換えた PNG を組む。画素は足さない
        // ——寸法を読んだ時点で断るので、そこまでしか読まれない。
        //
        // CRC を付け直す。 IHDR の検査が先に走るので、壊れたままだと
        // 「読めない」で落ちて、大きさの関門を通らない。
        let mut src = png(2, 2);
        let huge = 40_000_u32.to_be_bytes();
        src[16..20].copy_from_slice(&huge);
        src[20..24].copy_from_slice(&huge);
        let crc = crc32(&src[12..29]).to_be_bytes();
        src[29..33].copy_from_slice(&crc);

        let e = to_bmp(&src).expect_err("止まること");
        assert_eq!(e.code(), "icon.too_large");
        let e = to_portrait(&src).expect_err("止まること");
        assert_eq!(e.code(), "icon.too_large");
    }

    #[test]
    fn 読めない画像は失敗として返る() {
        let e = to_bmp("これは画像ではない".as_bytes()).expect_err("失敗すること");
        assert_eq!(e.code(), "icon.undecodable");
    }

    /// 透過は白へ畳む。黒く出る実装に当たらないようにする。
    #[test]
    fn 透過は白になる() {
        let mut img = image::RgbaImage::new(10, 10);
        for px in img.pixels_mut() {
            *px = image::Rgba([0, 0, 0, 0]);
        }
        let mut src = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut src, ImageFormat::Png)
            .expect("PNG を書けること");

        let bmp = to_bmp(&src.into_inner()).expect("変換できること");
        let pixels = &bmp[54..];
        assert!(
            pixels.iter().all(|b| *b == 255),
            "透過部分が白になっていない"
        );
    }
}
