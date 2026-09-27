//! 単一の出力枠（`T06d`、`DEC-REC-011`）。
//!
//! 音高提示・基準音・試唱・録れたテイクの再生・曲の試唱は、どれも同じ
//! 出力枠を取り合う。 新しい出力を始めるときは、必ず前の出力を止めてから
//! 始める——重ねると、いま聴いているのがどちらの音か分からなくなる。
//!
//! 曲の試唱（[`Output::Song`]）は単発の再生と違い、背後で合成し続ける
//! スレッド（[`Running`]）を持つ。 これを止める順序は、もともと
//! `Studio::stop_preview` が持っていたものと同じ——合図 → 再生 → 合成の
//! 待ち合わせ。 満杯のリングへの継ぎ足しで待っている合成スレッドは、
//! 再生（`stream`）を先に落とせば、その待ちが抜ける。 合成を先に待つと、
//! 鳴らして空くまで待つことになる。
//!
//! 落ちる順は宣言順に頼らず [`crate::capture_lease::DropFirst`] へ寄せてある
//! （そちらが持つ Pump → Capture の骨格そのもの）。 合図は `Drop` の本体で
//! `.second()` を借りて先に呼び、そのあと自然に `first`（再生）→
//! `second`（合成の待ち合わせ）の順で落ちる。
//!
//! 積んである仕事（`Workers::clear`）はここでは持たない。 出力の資源では
//! なく `Studio` が持つ仕事キューの話なので、境界を混ぜない
//! ——「曲を止めると、積んである合成の前処理も捨てる」という規則は
//! `Studio::stop_preview` 側にコメントで残す。

use koeru_audio::backend::current as mac;

use crate::capture_lease::DropFirst;
use crate::preview::Running;

/// いま鳴っている出力。1つだけ持てる。
#[derive(Debug)]
enum Output {
    /// 単発の再生（音高提示・基準音・試唱・テイクの再生・回り込みの検査）。
    ///
    /// 中身を読まない。 持つ理由は `Drop` で止まることだけ
    /// ——差し替え・`stop` で `Option` を書き換えると、古い方が自然に落ちる。
    #[allow(dead_code, reason = "RAII で持つだけ。中身は読まず Drop だけを使う")]
    Clip(mac::Playback),
    /// 継ぎ足しながら鳴らす曲の試唱（`TR-SYN-03`）。 落ちる順は再生 → 合成の待ち合わせ。
    Song(DropFirst<mac::Playback, Running>),
}

impl Drop for Output {
    fn drop(&mut self) {
        if let Self::Song(resources) = self {
            // 合図を先に立てる。 資源そのものは、この関数から戻ったあと
            // `resources`（`DropFirst`）が再生 → 合成の待ち合わせの順で落とす。
            resources.second().cancel();
        }
    }
}

/// 単一の出力枠。 `Studio` はこれを1つだけ持つ。
#[derive(Debug, Default)]
pub struct PlaybackLease {
    current: Option<Output>,
}

impl PlaybackLease {
    #[must_use]
    pub const fn new() -> Self {
        Self { current: None }
    }

    /// いまの出力を止める。 曲を歌わせていれば、その合成も止まる。
    ///
    /// 次の出力を作る前に呼ぶ（`mac::play` は失敗しうるので、成否に関わらず
    /// 前の出力を止めておきたい呼び出し側のために、差し替えとは別の手にしてある）。
    pub fn stop(&mut self) {
        self.current = None;
    }

    /// 単発の再生に差し替える。 前の出力を止めるのは呼び出し側の役目
    /// （[`Self::stop`]）——ここでは差し替えるだけ。
    pub fn set_clip(&mut self, playback: mac::Playback) {
        self.current = Some(Output::Clip(playback));
    }

    /// 曲の試唱に差し替える。 前提は [`Self::set_clip`] と同じ。
    pub fn set_song(&mut self, stream: mac::Playback, running: Running) {
        self.current = Some(Output::Song(DropFirst::new(stream, running)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn 新規のレースは何も鳴っていない() {
        let lease = PlaybackLease::new();
        assert!(lease.current.is_none());
    }

    #[test]
    // 見るのは代入そのものが前の値を落とすことで、`current` を読み直さない。
    #[allow(unused_assignments, reason = "再代入で前の値が落ちることを見る試験")]
    fn 差し替えると前の出力が落ちる() {
        // `mac::Playback` は実機が要るので作れない。 差し替えが前の値を
        // 落とすことは `Option` への再代入がふつうに持つ性質で、ここでは
        // その代入そのものを、実機を積む代わりに素の値で確かめる。
        let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        struct Spy(Arc<Mutex<Vec<&'static str>>>, &'static str);
        impl Drop for Spy {
            fn drop(&mut self) {
                self.0.lock().expect("ロックを取れる").push(self.1);
            }
        }
        let mut current = Some(Spy(Arc::clone(&log), "old"));
        current = Some(Spy(Arc::clone(&log), "new"));
        drop(current);
        assert_eq!(*log.lock().expect("ロックを取れる"), vec!["old", "new"]);
    }

    /// `Output::Song` が使う骨格——合図（cancel 相当）を先に立て、そのあと
    /// `DropFirst` の宣言順（`first` → `second`）で落ちること。
    ///
    /// 実際の `mac::Playback` / `Running` は実機が要るので、
    /// `crate::capture_lease::DropFirst` を汎用の資源で試す
    /// （`capture_lease` 自身の試験と同じ考え方）。
    #[test]
    fn 曲の出力は合図してから再生_合成の順で落ちる() {
        let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

        struct Spy(Arc<Mutex<Vec<&'static str>>>, &'static str);
        impl Drop for Spy {
            fn drop(&mut self) {
                self.0.lock().expect("ロックを取れる").push(self.1);
            }
        }

        /// `Running` の代わり。 `cancel()` を持ち、`Drop` でも同じことをする
        /// （`preview::Running` 自身の `Drop` が cancel してから join するのと同じ形）。
        struct FakeRunning(Arc<Mutex<Vec<&'static str>>>);
        impl FakeRunning {
            fn cancel(&self) {
                self.0.lock().expect("ロックを取れる").push("cancel");
            }
        }
        impl Drop for FakeRunning {
            fn drop(&mut self) {
                self.cancel();
                self.0.lock().expect("ロックを取れる").push("running");
            }
        }

        {
            let resources = DropFirst::new(
                Spy(Arc::clone(&log), "stream"),
                FakeRunning(Arc::clone(&log)),
            );
            // `Output::drop` がここで呼ぶもの。
            resources.second().cancel();
            // resources はここで落ち、`stream` → `FakeRunning`（cancel してから
            // 待ち合わせる）の順になる。
        }

        let seen = log.lock().expect("ロックを取れる").clone();
        assert_eq!(seen.first(), Some(&"cancel"), "合図が最初: {seen:?}");
        let stream_at = seen.iter().position(|s| *s == "stream").expect("stream");
        let running_at = seen.iter().position(|s| *s == "running").expect("running");
        assert!(
            stream_at < running_at,
            "stream が running より先に落ちる: {seen:?}"
        );
    }
}
