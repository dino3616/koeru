//! 音源アイコン（`TR-PKG-07`, `DEC-PKG-012`）。
//!
//! 本人が選んだ PNG / JPEG を 100×100 の BMP へ変換する。
//! BMP に固定するのは `TR-PKG-07` の判断で、classic UTAU が PNG を
//! 表示できるかを一次仕様として確認できていないため。

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, RgbImage, imageops::FilterType};

/// 音源アイコンの一辺（`TR-PKG-07`）。
pub const SIZE: u32 = 100;

/// アイコンを作れなかった理由。
#[derive(Debug, thiserror::Error)]
pub enum IconError {
    /// 画像として読めない。対応しているのは PNG と JPEG（`DEC-PKG-012`）。
    #[error("画像として読めない")]
    Undecodable {
        #[source]
        source: image::ImageError,
    },

    /// BMP として書けない。
    #[error("BMP として書けない")]
    Unencodable {
        #[source]
        source: image::ImageError,
    },
}

impl IconError {
    /// 送信してよい種別文字列。
    ///
    /// 画像そのものも、`Display` も送らない。画像は本人の創作物。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Undecodable { .. } => "icon.undecodable",
            Self::Unencodable { .. } => "icon.unencodable",
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
#[tracing::instrument(skip(source), fields(bytes = source.len()), err)]
pub fn to_bmp(source: &[u8]) -> Result<Vec<u8>> {
    let img =
        image::load_from_memory(source).map_err(|source| IconError::Undecodable { source })?;
    let square = img.resize_to_fill(SIZE, SIZE, FilterType::Lanczos3);

    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(flatten(&square))
        .write_to(&mut out, ImageFormat::Bmp)
        .map_err(|source| IconError::Unencodable { source })?;
    Ok(out.into_inner())
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

    #[test]
    fn 読めない画像は失敗として返る() {
        let e = to_bmp("これは画像ではない".as_bytes()).expect_err("失敗すること");
        assert_eq!(e.kind(), "icon.undecodable");
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
