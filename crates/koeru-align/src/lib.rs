//! 自動原音設定。
//!
//! 録音した WAV から oto の5値を機械で決める。 人が `setParam` で1件ずつ置いていた
//! 作業を、アライナに肩代わりさせる領域（`DEC-ALN-002`）。
//!
//! # 何をどの順で通すか
//!
//! ```text
//!  WAV ＋ 読み  →  アライメント  →  境界  →  5値の導出  →  確信度  →  確認キュー
//!                  （`TR-ALN-03`）           （`TR-ALN-13`〜18）（`TR-ALN-24`）（`TR-ALN-25`）
//! ```
//!
//! アライメントと5値の導出を分ける（`TR-ALN-13`）。前者は音響モデルが決め、
//! 後者は規約が決める。混ぜると、規約を変えるたびに推論をやり直すことになる。
//!
//! # 経路が2つある
//!
//! 一次経路は MFA の日本語音響モデル（`DEC-ALN-008`）。退避経路は
//! [`segment`] の音響モデルを使わない実装で、MFA が使えないときと、
//! MFA の統合が終わるまでの試唱に使う（`DEC-ALN-006`）。
//!
//! どちらも [`aligner::Aligner`] を実装する。 これは抽象を好むからではなく、
//! `TR-ALN-03` が「いずれの実装も emission 行列を呼び出し側に返し、
//! `TR-ALN-24` の確信度計算に使えること」と要求しているため。
//!
//! # 5値と確認の規則は `koeru-model` にある
//!
//! [`koeru_model::oto::Oto`] はプロジェクトのデータで、DB を正とする（`TR-PKG-40`）。
//! 制約（`TR-EDT-43`）は原音設定エディタも使う。ここが持つのは導出と規約。
//!
//! 確認キュー・検証と修復・到達水準は利用者の判断と書き出しの可否に関わるので、
//! 計算だけを持つこの crate から `koeru-model` へ移した（`DEC-PLT-034`）。 下の再輸出は
//! 既存の `koeru_align::review` などの経路を通すためだけにある。 **移行中。**

pub use koeru_model::{reach, review, validate};

pub mod aligner;
pub mod confidence;
pub mod consistency;
pub mod derive;
pub mod determinism;
pub mod ini;
pub mod ledger;
pub mod mfa;
pub mod phoneme;
pub mod preset;
pub mod resample;
pub mod segment;
pub mod subframe;
