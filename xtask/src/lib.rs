//! KOERU の仕様ゲート。
//!
//! FSL は形式的な契約の正本で、Decision / Question / Evidence / Budget / Profile は扱わない。
//! このツールはその外側だけを担当し、meta が FSL と技術要件の ID へ実際に繋がっているかを確かめる。
//! 仕様コンパイラではない。FSL のグラフへ外部情報を接続するブリッジとリリースゲートである。
//!
//! - `check-meta`     meta の形式と必須項目、参照先 ID の実在を確かめる
//! - `check-budgets`  配分の合計が上限を超えていないかを確かめる
//! - `check-profile`  未決の Question が塞いでいるリリースプロファイルを落とす
//! - `dump-requirements`  要件の登録簿を区切り文字形式で書き出す（外部ツール向け）
//! - `touched`        変更が触れた ID を、レビューに要る本文ごと出す
//! - `check-portfolio` 試験の target がどれも `meta/suites/` に登録されているか（`probe::receipt`）
//! - `test-receipt`   試験を走らせて件数を登録と突き合わせ、受領証を書く（`probe::receipt`）
//! - `check-schema`   canonical SDL と operation と能力の表を検査する（`checks::schema`）
//!
//! 置き場所は層で分ける。 下の層は上の層を引かない。
//!
//! - [`cli`] — 引数の振り分けと、コマンドに渡すもの（根と読んだ meta）の組み立て
//! - [`diagnostic`] — 検査の結果を集めることと、人に見せることの境界
//! - `repo` — リポジトリの根、走査から外す場所、git の呼び出し
//! - `knowledge` — meta と FSL を読んだもの。 形の宣言、読み込み、ID の拾い方
//! - `checks` — 合否を返す検査（`check-*`）
//! - `commands` — 合否を主にしないコマンド（ID の払い出し、索引、書き出し、差分の案内）
//! - `probe` — 試験を走らせて件数を数えるもの

// ここは CLI なので、結果を標準出力へ出す。tracing に寄せる対象ではない。
#![allow(clippy::print_stdout, clippy::print_stderr)]

pub mod cli;
pub mod diagnostic;

mod checks;
mod commands;
mod knowledge;
mod probe;
mod repo;
