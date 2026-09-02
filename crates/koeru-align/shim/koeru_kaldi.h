// KOERU から Kaldi を呼ぶための C 境界（TR-PLT-06）。
//
// **境界を跨ぐのは PCM バッファとパラメータ構造体だけ**（TR-PLT-06）。
// C++ の型は出さない。Rust 側は `mfa.rs` がここを包む。
//
// 返り値は 0 が成功、負が失敗。**失敗の理由は種別で返す**（AGENTS.md #3。
// パスも歌詞も載せない）。

#ifndef KOERU_KALDI_H_
#define KOERU_KALDI_H_

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>

// 失敗の種別。**Rust 側の `MfaError` と1対1。**
#define KOERU_OK                0
#define KOERU_ERR_MODEL       (-1)  // モデルを読めない
#define KOERU_ERR_ARGS        (-2)  // 引数が不正
#define KOERU_ERR_TOO_SHORT   (-3)  // 音声が音素列に対して短すぎる
#define KOERU_ERR_INTERNAL    (-4)  // Kaldi が例外を投げた

// モデル一式を保持する不透明ハンドル。
typedef struct KoeruKaldi KoeruKaldi;

// `model_dir` から final.mdl / final.alimdl / tree / lda.mat を読む。
// 失敗したら NULL。
KoeruKaldi* koeru_kaldi_open(const char* model_dir);
void koeru_kaldi_close(KoeruKaldi* h);

// 読んだモデルの特徴次元（LDA の出力次元。40 のはず）。
int koeru_kaldi_feature_dim(const KoeruKaldi* h);

// その音素を通過するのに要る最短フレーム数。**状態数とは限らない。**
int koeru_kaldi_min_length(const KoeruKaldi* h, int phone);

// 音素の総数（`phones.txt` の最大 id）。
int koeru_kaldi_num_phones(const KoeruKaldi* h);

// 特徴量を作る（MFCC → CMVN → splice(±3) → LDA）。
//
// `samples` は 16kHz モノラルの [-1, 1]。**リサンプルは呼び出し側の仕事**——
// ここで黙って変換すると「なぜか音が変」の原因が隠れる（TR-SYN-31 と同じ規律）。
//
// **`h` が const でないのは Kaldi の都合。** `Mfcc::ComputeFeatures` が非 const で、
// 内部に作業バッファを持っている。呼び出しでモデルは変わらない。
//
// `out` は NULL でもよい（必要なフレーム数を測るために呼べる）。
// 返り値はフレーム数、負なら失敗。
int koeru_kaldi_features(KoeruKaldi* h, const float* samples, int n_samples,
                         float* out, int out_capacity_frames);

// 1テイクを強制アライメントする（TR-ALN-03, TR-ALN-09）。
//
// `phone_ids` はモデル内の音素番号の列。**前後の無音は呼び出し側が入れない**——
// ここが `sil` を足す（TR-ALN-09 の (a)(b)「前後の無音区間の長さを自由にする」）。
//
// 出力:
//   `boundaries_ms`  長さ `n_phones + 3`（先頭 sil ＋ 音素列 ＋ 末尾 sil の境界）。
//                    **区間の境目をミリ秒で、先頭 0 から順に。**
//   `log_likelihood` 音素列全体の対数尤度（TR-ALN-09 (c) のテキスト逸脱の判定に使う）
//   `posteriors`     フレーム × (n_phones + 2) の行優先。NULL なら書かない（TR-ALN-03）
//   `n_frames`       フレーム数
//
// **`posteriors` の容量は `koeru_kaldi_features` で測ったフレーム数 × (n_phones+2)。**
//
// 返り値 0 が成功。
int koeru_kaldi_align(KoeruKaldi* h, const float* samples, int n_samples,
                      const int* phone_ids, int n_phones,
                      float* boundaries_ms, float* log_likelihood,
                      float* posteriors, int posteriors_capacity, int* n_frames);

#ifdef __cplusplus
}
#endif

#endif  // KOERU_KALDI_H_
