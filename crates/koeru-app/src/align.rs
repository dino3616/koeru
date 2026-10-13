//! アライナを用意する（`TR-ALN-03`, `DEC-ALN-008`）。
//!
//! MFA だけ（`DEC-ALN-016`）。 音響モデルを使わない退避経路は持たない。
//!
//! # モデルが無いのはエラー
//!
//! モデルは配布物に同梱している（`DEC-ALN-012`）。 開発でも submodule を
//! 取ってくるのが前提（`setup-koeru`）。**だからモデルが読めないのは
//! ビルドの失敗であって、動かしてよい状態ではない。**
//!
//! 退避で埋めない。 実際に埋めて見えなくなっていた——macOS の配布物で
//! 資源の場所を1箇所しか探しておらず、**全員が黙って退避経路で動いていた。**
//!
//! 起動で落とす。 [`Chosen::detect`] は失敗を返し、`Studio::open` がそれを
//! 上げる。収録の途中で「原音設定だけ出ない」ことに気づく形にしない。
//!
//! # モデルの置き場所
//!
//! リポジトリに submodule で同梱している（`DEC-ALN-012`）。探す順は3つ。
//!
//! 1. 環境変数 `KOERU_MFA_MODEL_DIR`（開発中の差し替え用）
//! 2. 配布物の資源置き場（`tauri.conf.json` の `bundle.resources`）
//! 3. リポジトリの `crates/koeru-align/models/japanese_mfa/acoustic`（`cargo run` のとき）
//!
//! どこにも無ければ使えない（[`availability`]）。実行時に取りに行かない
//! （`TR-PLT-19` の「初回起動後の追加ダウンロードをゼロにする」）。
//!
//! **探すのはここで、エンジン（`koeru-align`）は探さない**（`DEC-PLT-034`）。 環境変数と
//! ファイルシステムを読む口がエンジンにあると、同じ入力から同じ答えを返す計算の外に
//! 置き場所という入力が混ざる。 エンジンが受け取るのは、ここが見つけたディレクトリと、
//! `meta.json` の中身の文字列だけ。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use koeru_align::aligner::{
    AlignError, AlignRequest, Aligner, Alignment, Availability, Unavailable,
};
use koeru_align::mfa::MfaAligner;
use koeru_failure::Class;

use crate::error::AppError;

/// 見つからないときの code（`DEC-ALN-016`）。 梱包が壊れているか、submodule を取っていない。
const MODEL_NOT_FOUND: &str = "align.model_not_found";

/// モデルを差し替える環境変数。 開発中に別の版を当てるときに使う。
const MODEL_DIR_ENV: &str = "KOERU_MFA_MODEL_DIR";

/// submodule で抱えているモデルの位置（`DEC-ALN-012`）。 `koeru-app` の `Cargo.toml` からの相対。
const MODEL_DIR_IN_REPO: &str = "../koeru-align/models/japanese_mfa/acoustic";

/// 配布物の資源置き場（実行ファイルからの相対）。
///
/// **OS で並びが違う。** Tauri が資源を置くのは
/// macOS なら `Contents/Resources/`（実行ファイルは `Contents/MacOS/`）、
/// Windows と Linux なら実行ファイルの隣。両方見る。
///
/// 片方だけ見ていた。 実行ファイルの隣しか探していなかったので、
/// **macOS の配布物では同梱したモデルが見つからず、全員が黙って
/// 退避経路で動くことになる。**
const MODEL_DIRS_RELATIVE: [&str; 2] = ["models/japanese_mfa", "../Resources/models/japanese_mfa"];

/// 用意できたアライナ（`DEC-ALN-016`）。
///
/// **必ず在る。** 無い状態を型で表さない——無いのはビルドの失敗で、
/// そこまで来たら起動が止まっている（[`Chosen::detect`]）。
///
/// 複製は持ち手の複製で、モデルは1つ。 計算を別のスレッドへ渡すときは、これを渡す。
///
/// **`MfaAligner` は `Send` だが `Sync` ではない。** Kaldi の `Mfcc` が作業バッファを
/// 持っていて、同時に呼ぶと壊れる。 `Arc` だけで共有すると `!Sync` が `Arc` ごと
/// スレッドを渡れなくするので、`Mutex` で包んで同時に呼べない形にしてある。
#[derive(Debug, Clone)]
pub struct Chosen {
    inner: Arc<Mutex<Box<MfaAligner>>>,
    /// 何で推定したかの札（`TR-ALN-29`）。 モデルを包んだ鍵を待たずに引ける。
    identity: Arc<str>,
}

impl Chosen {
    /// MFA のモデルを読む。
    ///
    /// **読めなければ失敗を返す**（`DEC-ALN-016`）。 同梱されているはずのものが
    /// 無いのはビルドの失敗で、そのまま動かすと退避も無いまま原音設定だけが
    /// 出ないことになる。起動で止める。
    ///
    /// # Errors
    ///
    /// モデルが見つからない、または読めないとき。
    pub fn detect() -> Result<Self, AppError> {
        // 使えるかを先に値で問う。 止めるかどうかは、ここ（起動）の方針（`DEC-ALN-016`）。
        let dir = locate().map_err(unavailable_error)?;
        // 識別子はモデルに名乗らせる（`TR-ALN-29`）。定数で持つと、
        // submodule を上げたときに指紋だけが古い版を指す。
        let identity = koeru_align::mfa::model_identity(read_model_version(&dir).as_deref());
        match MfaAligner::open(&dir, &identity) {
            Ok(a) => {
                tracing::info!(dim = a.feature_dim(), "自動原音設定は MFA で動く");
                Ok(Self {
                    identity: Arc::from(a.identity()),
                    inner: Arc::new(Mutex::new(Box::new(a))),
                })
            }
            // パスは載せない（AGENTS.md #3）。種別だけ。
            Err(e) => {
                Err(AppError::from_failure(e).saying("MFA のモデルを読めない。同梱物が壊れている"))
            }
        }
    }

    /// 1テイクをアライメントする。 同時には1つしか走らない。
    ///
    /// # Errors
    ///
    /// [`Aligner::align`] と同じ。
    pub fn align(&self, req: &AlignRequest<'_>) -> Result<Alignment, AlignError> {
        // 鍵が毒されていても取り出して使う。 `MfaAligner` は Rust 側に守る不変条件を
        // 持たない（`koeru_align::mfa` の `Send` の注を参照）。
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .align(req)
    }

    /// 何で推定したかの札（`TR-ALN-29`）。
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

/// MFA を使えるか（`DEC-PLT-038`）。 失敗ではなく値で返す。
///
/// 先に OS（Kaldi を組んでいるか）、次にモデルが置いてあるか。 起動で止める
/// （`Studio::open`、`DEC-ALN-016`）かどうかは呼び出し側の方針で、ここは答えるだけ。
#[must_use]
pub fn availability() -> Availability {
    match locate() {
        Ok(_) => Availability::Available,
        Err(reason) => Availability::Unavailable(reason),
    }
}

/// 使えるならモデルの場所を、使えないなら理由を返す。
fn locate() -> Result<PathBuf, Unavailable> {
    if let Some(reason) = koeru_align::mfa::availability().reason() {
        return Err(reason);
    }
    model_dir().ok_or(Unavailable {
        code: MODEL_NOT_FOUND,
        class: Class::Unsupported,
    })
}

/// 使えない理由を、境界の失敗へ写す。 文言は固定で、パスは載せない（`AGENTS.md` の禁止事項3）。
fn unavailable_error(reason: Unavailable) -> AppError {
    let message = if reason.code == MODEL_NOT_FOUND {
        "MFA のモデルが見つからない。submodule を取り込んでいるか確かめてほしい"
    } else {
        "この OS の自動原音設定はまだ書いていない"
    };
    AppError::new(reason.code, reason.class, message)
}

/// そこに音響モデルがあるか。 ディレクトリの存在では見ない。
///
/// LFS を入れずに clone すると、`final.mdl` は 130 バイト程度のポインタで置かれる。
/// ファイルはあるので `is_file` は通り、Kaldi が読む段になって初めて落ちる。
///
/// **探す経路はどれもこれを通す。** 配布物側だけ `is_file` で見ていたので、
/// ポインタのまま同梱された配布物が選ばれ、MFA が黙って退避経路へ落ちていた。
fn has_model(dir: &Path) -> bool {
    std::fs::metadata(dir.join("final.mdl")).is_ok_and(|m| m.is_file() && m.len() > 1_000_000)
}

/// モデルが自分で名乗る版（`meta.json` の `version`）。 読めなければ `None`。
///
/// 読むのはここで、文字列の解釈は `koeru-align` が持つ（`TR-ALN-29`）。
fn read_model_version(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("meta.json")).ok()?;
    koeru_align::mfa::model_version(&text)
}

/// モデルの置き場所を探す。先に見つかったものを使う。
fn model_dir() -> Option<PathBuf> {
    env_model_dir()
        .or_else(exe_model_dir)
        .or_else(repo_model_dir)
}

/// 環境変数が指すモデル。
fn env_model_dir() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var(MODEL_DIR_ENV).ok()?);
    has_model(&p).then_some(p)
}

/// submodule の中のモデル。 `cargo run` のときに見つかる。 submodule を取っていなければ `None`。
fn repo_model_dir() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MODEL_DIR_IN_REPO);
    has_model(&p).then_some(p)
}

/// 配布物の中（`tauri.conf.json` の `bundle.resources` が置く場所）。
fn exe_model_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    MODEL_DIRS_RELATIVE
        .into_iter()
        .map(|rel| dir.join(rel))
        .find(|p| has_model(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MFA を組んである OS では、モデルが読めて札を名乗る（`DEC-ALN-016`）。
    ///
    /// submodule は取ってあるのが前提（`setup-koeru`）。 通らない環境は、
    /// まず submodule を取る——**退避で埋めない。**
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn モデルが読めれば札を名乗る() {
        let c = Chosen::detect().expect("submodule を取り込んでいれば読める");
        assert!(!c.identity().is_empty(), "何で推定したかの札を持つ");
    }

    /// MFA を組んでいない OS では、名指しで失敗する（`DEC-ALN-016`）。
    ///
    /// **黙って動かない。** 退避も持たないので、自動原音設定が無いまま
    /// 収録だけ進む形を作らない。`Studio::open` がこれを上げて起動が止まる。
    ///
    /// `TR-PLT-01` は Windows を第一級の対象としているが、Kaldi の移植が
    /// 終わるまでは動かない。**その衝突を隠さないための試験。**
    #[cfg(any(not(target_os = "macos"), koeru_force_unsupported_backend))]
    #[test]
    fn 組んでいない_os_では名指しで失敗する() {
        let e = Chosen::detect().expect_err("この OS には Kaldi を組んでいない");
        assert_eq!(e.code, "mfa.unsupported_platform");
    }

    /// 組んでいない OS では、使えないと値で言う。 理由は起動で上がる失敗と同じ code と分類
    /// （`DEC-PLT-038`）。 モデルが置いてあっても、OS の理由が先に立つ。
    #[cfg(any(not(target_os = "macos"), koeru_force_unsupported_backend))]
    #[test]
    fn 組んでいない_os_では使えないと値で言う() {
        assert_eq!(
            availability(),
            Availability::Unavailable(Unavailable {
                code: "mfa.unsupported_platform",
                class: Class::Unsupported,
            })
        );
        assert!(model_dir().is_some(), "前提: モデルは置いてある");
    }

    /// Kaldi を組んである OS では、モデルが置いてあれば使えると値で言う。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 組んである_os_では使えると値で言う() {
        assert_eq!(availability(), Availability::Available);
    }

    /// 使えない理由を失敗へ写しても、code と分類が変わらない。 文言にパスを載せない。
    #[test]
    fn 使えない理由は固定の文言の失敗になる() {
        for (code, class) in [
            (MODEL_NOT_FOUND, Class::Unsupported),
            ("mfa.unsupported_platform", Class::Unsupported),
        ] {
            let e = unavailable_error(Unavailable { code, class });
            assert_eq!(e.code, code);
            assert!(!e.message.contains('/'), "{}", e.message);
        }
    }

    /// submodule を初期化していれば、環境変数なしでモデルが見つかる（`DEC-ALN-012`）。
    ///
    /// 探す経路の試験で、アライナを組むかどうかとは別。 モデルが無ければ落とす
    /// ——戻ると何も見ずに通る（`DEC-PLT-039`）。
    #[test]
    fn リポジトリの中のモデルを見つけられる() {
        let dir = repo_model_dir()
            .expect("MFA のモデルが無い。submodule と LFS を取り込む（setup-koeru）");
        // 環境変数を使わずに見つかること。
        assert!(model_dir().is_some());
        // 版はモデル自身が名乗る（`TR-ALN-29`）。 読めなければ識別子が `unknown` になる。
        assert!(read_model_version(&dir).is_some(), "meta.json の version");
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("koeru-align-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("作れる");
        dir
    }

    /// LFS のポインタ（130 バイト程度）のまま置かれたものはモデルとして数えない。
    #[test]
    fn ポインタのままのモデルは見つけたことにしない() {
        let dir = tmp("pointer");
        std::fs::write(
            dir.join("final.mdl"),
            b"version https://git-lfs.github.com/spec/v1\n",
        )
        .expect("書ける");
        assert!(!has_model(&dir));
        assert!(!has_model(&dir.join("none")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 版は置いてある `meta.json` から読む。 無ければ `None`（識別子は `unknown`）。
    #[test]
    fn 版は_meta_json_から読む() {
        let dir = tmp("meta");
        assert_eq!(read_model_version(&dir), None);
        std::fs::write(dir.join("meta.json"), r#"{"version": "9.9.9"}"#).expect("書ける");
        assert_eq!(read_model_version(&dir).as_deref(), Some("9.9.9"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
