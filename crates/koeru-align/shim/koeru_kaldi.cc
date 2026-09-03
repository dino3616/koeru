// KOERU から Kaldi を呼ぶための C 境界（TR-PLT-06）。
//
// **ここが Kaldi の C++ と Rust の唯一の接点。** C++ の例外はここで止め、
// 種別コードに畳んで返す（Rust 側へ例外を投げると未定義動作になる）。
//
// 特徴パイプラインは EVID-ALN-001 の実測どおり:
//   MFCC(13) → CMVN → splice(±3)=91 → LDA+MLLT(40x91) → 40
//
// **`meta.json` の `uses_splices` / `uses_deltas` はこのモデルの実態を表していない。**
// `final.mdl` の `<DIMENSION>` が 40、`lda.mat` が 40x91 で、Δ+ΔΔ の 39 ではない。

#include "koeru_kaldi.h"

#include <cmath>
#include <limits>
#include <string>
#include <vector>

#include "base/kaldi-common.h"
#include "feat/feature-mfcc.h"
#include "gmm/am-diag-gmm.h"
#include "hmm/transition-model.h"
#include "matrix/kaldi-matrix.h"
#include "gmm/decodable-am-diag-gmm.h"
#include "hmm/hmm-topology.h"
#include "transform/cmvn.h"
#include "transform/fmllr-diag-gmm.h"
#include "tree/context-dep.h"
#include "util/common-utils.h"

namespace {

// モデルの想定サンプリング周波数（EVID-ALN-001。meta.json の `sample_frequency`）。
constexpr float kSampleRateHz = 16000.0f;

// splice の文脈幅（meta.json の `splice_left_context` / `splice_right_context`）。
constexpr int kSpliceLeft = 3;
constexpr int kSpliceRight = 3;

// フレーム進み幅（ミリ秒）。**TR-ALN-06 の 2ms はサブフレーム補間で作る。**
constexpr float kFrameShiftMs = 10.0f;

// 無音の音素番号（phones.txt の `sil`）。**`<eps>` が 0、`sil` が 1、`spn` が 2。**
constexpr int32 kSilencePhone = 1;

// 対数領域での足し算。**大きいほうを括り出して桁落ちを避ける。**
inline float LogAdd(float a, float b) {
  if (a == -std::numeric_limits<float>::infinity()) return b;
  if (b == -std::numeric_limits<float>::infinity()) return a;
  const float hi = a > b ? a : b;
  const float lo = a > b ? b : a;
  return hi + std::log1p(std::exp(lo - hi));
}

// MFCC の設定（EVID-ALN-001 の meta.json より。**値を勝手に変えない**——
// 学習時と違う設定で特徴を作ると、モデルが別物を見ることになる）。
kaldi::MfccOptions MakeMfccOptions() {
  kaldi::MfccOptions o;
  o.frame_opts.samp_freq = kSampleRateHz;
  o.frame_opts.frame_shift_ms = 10.0f;
  o.frame_opts.frame_length_ms = 25.0f;
  o.frame_opts.snip_edges = false;
  // **dither を切る。** meta.json は `dither: 1` だが、dither は各フレームへ
  // 乱数の雑音を足す処理で、**TR-ALN-29 の「ビット単位で同一」と両立しない。**
  // 学習時の設定と1つずれるが、dither はモデルの選択ではなく数値安定のための
  // 措置（log(0) を避ける）で、Kaldi 自身がメル帯域のエネルギーに
  // `ApplyFloor(epsilon)` を掛けているので、切っても 0 の log は踏まない。
  // **決定性を要件が求めている以上、ここは切るしかない。**
  o.frame_opts.dither = 0.0f;
  o.frame_opts.preemph_coeff = 0.97f;
  o.mel_opts.num_bins = 23;
  o.mel_opts.low_freq = 20.0f;
  o.mel_opts.high_freq = 7800.0f;
  o.num_ceps = 13;
  o.cepstral_lifter = 22.0f;
  o.use_energy = false;
  return o;
}

}  // namespace

struct KoeruKaldi {
  kaldi::TransitionModel trans_model;   // final.mdl の遷移モデル
  kaldi::AmDiagGmm am_gmm;              // final.mdl の GMM（SAT）
  kaldi::TransitionModel ali_trans;     // final.alimdl の遷移モデル
  kaldi::AmDiagGmm ali_gmm;             // final.alimdl の GMM（話者非依存）
  kaldi::ContextDependency ctx_dep;     // tree
  kaldi::Matrix<float> lda;             // lda.mat（40 x 91）
  kaldi::Mfcc mfcc{MakeMfccOptions()};
};

namespace {

// Kaldi の `.mdl` は「遷移モデル ＋ 音響モデル」が並んで入っている。
void ReadModel(const std::string& path, kaldi::TransitionModel* tm,
               kaldi::AmDiagGmm* am) {
  bool binary = false;
  kaldi::Input ki(path, &binary);
  tm->Read(ki.Stream(), binary);
  am->Read(ki.Stream(), binary);
}

}  // namespace


namespace {


// 特徴量を作る（MFCC → CMVN → splice(±3) → LDA）。**`align` と `features` が共有する。**
int ComputeFeatures(KoeruKaldi* h, const float* samples, int n_samples,
                    kaldi::Matrix<float>* out) {
  kaldi::Vector<float> wave(n_samples);
  for (int i = 0; i < n_samples; ++i) {
    // Kaldi は 16bit の目盛りで振幅を扱う。**[-1,1] から合わせる。**
    wave(i) = samples[i] * 32768.0f;
  }

  kaldi::Matrix<float> mfcc;
  h->mfcc.ComputeFeatures(wave, kSampleRateHz, 1.0f, &mfcc);
  if (mfcc.NumRows() == 0) return KOERU_ERR_TOO_SHORT;

  // CMVN。**平均だけを揃える。分散は触らない。**
  //
  // `meta.json` は `uses_cmvn: true` としか書いていないが、
  // **Kaldi の `apply-cmvn` は既定で平均のみ**（`--norm-vars=false`）で、
  // モデルもその前提で学習されている。
  //
  // **分散まで正規化すると、アライメントが壊れる。** 実測した——
  // 6.3秒の録音（発声は 1590〜2630ms）で、8音素が 2025〜2105ms に
  // 各10ms で潰れた。**次元ごとに分散を1へ揃えると特徴の尺度が変わり、
  // 広い分散を持つ無音モデルがどのフレームでも勝つ。**
  // 平均のみに直したら、境界が発声の範囲とほぼ一致した。**踏んだ。**
  kaldi::Matrix<double> stats;
  kaldi::InitCmvnStats(mfcc.NumCols(), &stats);
  for (int32 r = 0; r < mfcc.NumRows(); ++r) {
    kaldi::AccCmvnStats(mfcc.Row(r), 1.0, &stats);
  }
  kaldi::ApplyCmvn(stats, /*var_norm=*/false, &mfcc);

  // splice(±3)。**端はフレームを複製して埋める**（Kaldi の既定と同じ）。
  const int32 dim = mfcc.NumCols();
  const int32 n = mfcc.NumRows();
  const int32 spliced_dim = dim * (kSpliceLeft + 1 + kSpliceRight);
  if (spliced_dim != h->lda.NumCols()) return KOERU_ERR_MODEL;

  kaldi::Matrix<float> spliced(n, spliced_dim);
  for (int32 r = 0; r < n; ++r) {
    for (int32 o = -kSpliceLeft, k = 0; o <= kSpliceRight; ++o, ++k) {
      int32 src = r + o;
      if (src < 0) src = 0;
      if (src >= n) src = n - 1;
      spliced.Range(r, 1, k * dim, dim).CopyFromMat(mfcc.Range(src, 1, 0, dim));
    }
  }

  // LDA+MLLT。**40 x 91 を掛けて 40 次にする。**
  out->Resize(n, h->lda.NumRows());
  out->AddMatMat(1.0f, spliced, kaldi::kNoTrans, h->lda, kaldi::kTrans, 0.0f);
  return KOERU_OK;
}

// 強制アライメントのグラフの1ノード。
//
// **左から右へのチェーン。** 単独音は音素列が [sil, C, V, sil] のように短く、
// OpenFst の一般的な合成が要らない（build.rs の説明を参照）。
struct Node {
  int32 phone_slot;   // 音素列の何番目か（0 が先頭の sil）
  int32 hmm_state;    // その音素の中の HMM 状態
  int32 pdf;          // 出力分布
  int32 self_loop_tid;  // 自己ループの transition-id（無ければ 0）
  std::vector<std::pair<int32, float>> arcs;  // (次ノード, 対数遷移確率)
};

// 音素列からグラフを組む。**三音素の文脈は列から取る。**
bool BuildGraph(const kaldi::TransitionModel& tm,
                const kaldi::ContextDependency& ctx,
                const std::vector<int32>& phones,
                std::vector<Node>* nodes) {
  const kaldi::HmmTopology& topo = tm.GetTopo();
  const int32 width = ctx.ContextWidth();
  const int32 central = ctx.CentralPosition();

  // 各音素の開始ノード番号を先に決める。
  std::vector<int32> start(phones.size() + 1, 0);
  for (size_t i = 0; i < phones.size(); ++i) {
    const auto& entry = topo.TopologyForPhone(phones[i]);
    // 末尾の非出力状態は数えない（Kaldi の topology は最後が終端）。
    start[i + 1] = start[i] + static_cast<int32>(entry.size()) - 1;
  }
  nodes->assign(start[phones.size()], Node());

  for (size_t i = 0; i < phones.size(); ++i) {
    const auto& entry = topo.TopologyForPhone(phones[i]);
    // **文脈窓。** 列の外は 0（Kaldi が「音素なし」として扱う番号）。
    std::vector<int32> window(width, 0);
    for (int32 k = 0; k < width; ++k) {
      const int64 idx = static_cast<int64>(i) + k - central;
      if (idx >= 0 && idx < static_cast<int64>(phones.size())) {
        window[k] = phones[idx];
      }
    }

    for (size_t st = 0; st + 1 < entry.size(); ++st) {
      Node& n = (*nodes)[start[i] + st];
      n.phone_slot = static_cast<int32>(i);
      n.hmm_state = static_cast<int32>(st);

      int32 forward_pdf = 0, self_pdf = 0;
      if (!ctx.Compute(window, entry[st].forward_pdf_class, &forward_pdf)) return false;
      if (!ctx.Compute(window, entry[st].self_loop_pdf_class, &self_pdf)) return false;
      n.pdf = forward_pdf;

      const int32 ts = tm.TupleToTransitionState(phones[i], static_cast<int32>(st),
                                                 forward_pdf, self_pdf);
      for (size_t ti = 0; ti < entry[st].transitions.size(); ++ti) {
        const int32 dest = entry[st].transitions[ti].first;
        const int32 tid = tm.PairToTransitionId(ts, static_cast<int32>(ti));
        const float lp = tm.GetTransitionLogProb(tid);
        if (dest == static_cast<int32>(st)) {
          n.self_loop_tid = tid;
          n.arcs.emplace_back(start[i] + st, lp);       // 自分へ戻る
        } else if (dest + 1 == static_cast<int32>(entry.size())) {
          // その音素の終端。**次の音素の先頭へ繋ぐ。**
          //
          // **最後の音素の出口は繋がない。** `start[phones.size()]` はノード数そのもので、
          // 繋ぐと DP 配列の外を指す。経路はグラフの終端ノードで終わる。
          if (i + 1 < phones.size()) {
            n.arcs.emplace_back(start[i + 1], lp);
          }
        } else {
          n.arcs.emplace_back(start[i] + dest, lp);
        }
      }
    }
  }
  return true;
}


// 与えたグラフと出力尤度で Viterbi を回し、フレームごとのノードを返す。
//
// **戻り値は経路全体の対数尤度。** 終端へ届かなければ `-inf`。
float Viterbi(const std::vector<Node>& nodes, const std::vector<float>& emit, int32 T,
              std::vector<int32>* path) {
  constexpr float kNegInf = -std::numeric_limits<float>::infinity();
  const int32 N = static_cast<int32>(nodes.size());
  std::vector<float> delta(static_cast<size_t>(T) * N, kNegInf);
  std::vector<int32> back(static_cast<size_t>(T) * N, -1);
  delta[0] = emit[0];
  for (int32 t = 1; t < T; ++t) {
    const float* prev = &delta[static_cast<size_t>(t - 1) * N];
    float* cur = &delta[static_cast<size_t>(t) * N];
    int32* bp = &back[static_cast<size_t>(t) * N];
    for (int32 i = 0; i < N; ++i) {
      if (prev[i] == kNegInf) continue;
      for (const auto& a : nodes[i].arcs) {
        const float score = prev[i] + a.second;
        if (score > cur[a.first]) {
          cur[a.first] = score;
          bp[a.first] = i;
        }
      }
    }
    for (int32 i = 0; i < N; ++i) {
      if (cur[i] != kNegInf) cur[i] += emit[static_cast<size_t>(t) * N + i];
    }
  }
  const float best = delta[static_cast<size_t>(T - 1) * N + (N - 1)];
  if (best == kNegInf) return best;

  path->assign(T, 0);
  (*path)[T - 1] = N - 1;
  for (int32 t = T - 1; t > 0; --t) {
    (*path)[t - 1] = back[static_cast<size_t>(t) * N + (*path)[t]];
    if ((*path)[t - 1] < 0) return kNegInf;
  }
  return best;
}

// 出力対数尤度を全フレーム分作る。**同じ pdf を何度も引かない。**
void Emissions(const kaldi::AmDiagGmm& am, const std::vector<Node>& nodes,
               const kaldi::Matrix<float>& feats, std::vector<float>* emit) {
  const int32 T = feats.NumRows();
  const int32 N = static_cast<int32>(nodes.size());
  emit->assign(static_cast<size_t>(T) * N, 0.0f);
  for (int32 t = 0; t < T; ++t) {
    for (int32 i = 0; i < N; ++i) {
      (*emit)[static_cast<size_t>(t) * N + i] =
          am.LogLikelihood(nodes[i].pdf, feats.Row(t));
    }
  }
}

// 1パス目の経路から fMLLR 行列を推定する（EVID-ALN-001 の2パス構成）。
//
// **話者はテイクの発声者1人。** 1プロジェクト＝1話者なので、
// このテイクの中だけで推定する（TR-ALN-12 の集団統計とは別の話）。
//
// 推定できなければ false（そのときは1パス目の結果をそのまま使う）。
bool EstimateFmllr(const kaldi::AmDiagGmm& am, const kaldi::TransitionModel& tm,
                   const std::vector<Node>& nodes, const std::vector<int32>& path,
                   const kaldi::Matrix<float>& feats, kaldi::Matrix<float>* xform) {
  const int32 dim = feats.NumCols();
  kaldi::FmllrDiagGmmAccs accs(dim);
  kaldi::Vector<float> one(1);
  one(0) = 1.0f;

  for (size_t t = 0; t < path.size(); ++t) {
    const int32 pdf = nodes[path[t]].pdf;
    const kaldi::DiagGmm& gmm = am.GetPdf(pdf);
    accs.AccumulateForGmm(gmm, feats.Row(static_cast<int32>(t)), 1.0f);
  }
  (void)tm;

  // 単位行列から始める。**推定が進まなければ単位のまま返る。**
  xform->Resize(dim, dim + 1);
  xform->SetZero();
  for (int32 i = 0; i < dim; ++i) (*xform)(i, i) = 1.0f;

  // **`ComputeFmllrDiagGmm` は使えない。** ヘッダに宣言はあるが、
  // Kaldi のどこにも定義が無い（上流の死んだ宣言。リンクで初めて分かる）。
  // 実際の口は `FmllrDiagGmmAccs::Update`。
  kaldi::FmllrOptions opts;
  float objf_impr = 0.0f, count = 0.0f;
  accs.Update(opts, xform, &objf_impr, &count);
  // **改善が無ければ2パス目を回さない。** 単位行列のまま SAT を当てると尤度が歪む。
  return count > 0.0f && objf_impr > 0.0f;
}

}  // namespace

extern "C" {

KoeruKaldi* koeru_kaldi_open(const char* model_dir) {
  if (model_dir == nullptr) return nullptr;
  try {
    const std::string dir(model_dir);
    auto* h = new KoeruKaldi();
    ReadModel(dir + "/final.mdl", &h->trans_model, &h->am_gmm);
    ReadModel(dir + "/final.alimdl", &h->ali_trans, &h->ali_gmm);
    {
      bool binary = false;
      kaldi::Input ki(dir + "/tree", &binary);
      h->ctx_dep.Read(ki.Stream(), binary);
    }
    {
      bool binary = false;
      kaldi::Input ki(dir + "/lda.mat", &binary);
      h->lda.Read(ki.Stream(), binary);
    }
    return h;
  } catch (...) {
    // **例外を Rust へ投げない。** 境界で止めて NULL にする。
    return nullptr;
  }
}

void koeru_kaldi_close(KoeruKaldi* h) { delete h; }

int koeru_kaldi_feature_dim(const KoeruKaldi* h) {
  if (h == nullptr) return KOERU_ERR_ARGS;
  return h->lda.NumRows();
}

// その音素を通過するのに要る最短フレーム数（`HmmTopology::MinLength`）。
//
// **状態数と違うことがある。** topology が飛び越しを許していれば短くなる。
int koeru_kaldi_min_length(const KoeruKaldi* h, int phone) {
  if (h == nullptr || phone <= 0) return KOERU_ERR_ARGS;
  try {
    // **最短の継続長も返せるようにする。** 状態数と最短長がずれていたら、
    // topology が飛び越しを許している（それなら1フレームで通過できる）。
    return h->trans_model.GetTopo().MinLength(phone);
  } catch (...) {
    return KOERU_ERR_ARGS;
  }
}

int koeru_kaldi_num_phones(const KoeruKaldi* h) {
  if (h == nullptr) return KOERU_ERR_ARGS;
  return h->trans_model.NumPhones();
}

int koeru_kaldi_features(KoeruKaldi* h, const float* samples, int n_samples,
                         float* out, int out_capacity_frames) {
  if (h == nullptr || samples == nullptr || n_samples <= 0) return KOERU_ERR_ARGS;
  try {
    kaldi::Matrix<float> feats;
    const int rc = ComputeFeatures(h, samples, n_samples, &feats);
    if (rc != KOERU_OK) return rc;
    const int32 n = feats.NumRows();
    if (out != nullptr) {
      if (out_capacity_frames < n) return KOERU_ERR_ARGS;
      for (int32 r = 0; r < n; ++r) {
        for (int32 c = 0; c < feats.NumCols(); ++c) {
          out[r * feats.NumCols() + c] = feats(r, c);
        }
      }
    }
    return n;
  } catch (...) {
    return KOERU_ERR_INTERNAL;
  }
}


int koeru_kaldi_align(KoeruKaldi* h, const float* samples, int n_samples,
                      const int* phone_ids, int n_phones,
                      float* boundaries_ms, float* log_likelihood,
                      float* posteriors, int posteriors_capacity, int* n_frames) {
  if (h == nullptr || samples == nullptr || phone_ids == nullptr || n_phones <= 0 ||
      boundaries_ms == nullptr || n_frames == nullptr) {
    return KOERU_ERR_ARGS;
  }
  try {
    // 1) 特徴量。
    kaldi::Matrix<float> feats;
    const int rc = ComputeFeatures(h, samples, n_samples, &feats);
    if (rc != KOERU_OK) return rc;
    const int32 T = feats.NumRows();
    *n_frames = T;

    // 2) 音素列の前後に `sil` を足す（TR-ALN-09 の (a)(b)）。
    //    **前後の無音の長さは自由。** 自己ループで何フレームでも伸ばせる。
    std::vector<int32> phones;
    phones.reserve(n_phones + 2);
    phones.push_back(kSilencePhone);
    for (int i = 0; i < n_phones; ++i) {
      if (phone_ids[i] <= 0) return KOERU_ERR_ARGS;
      phones.push_back(phone_ids[i]);
    }
    phones.push_back(kSilencePhone);

    // **1パス目は話者非依存モデル（`final.alimdl`）を使う。**
    // `final.mdl` は SAT で、fMLLR を掛けた特徴を前提にしている。素の特徴に当てると
    // 尤度が歪む（EVID-ALN-001 の2パス構成）。**2パス目は 4) で回す。**
    std::vector<Node> nodes;
    if (!BuildGraph(h->ali_trans, h->ctx_dep, phones, &nodes)) return KOERU_ERR_MODEL;
    if (T < static_cast<int32>(nodes.size())) {
      return KOERU_ERR_TOO_SHORT;  // 状態の数だけフレームが要る
    }

    // 3) 1パス目。**話者非依存モデル**（`final.alimdl`）で粗く合わせる。
    std::vector<float> emit;
    Emissions(h->ali_gmm, nodes, feats, &emit);

    std::vector<int32> path;
    float best = Viterbi(nodes, emit, T, &path);
    if (best == -std::numeric_limits<float>::infinity()) return KOERU_ERR_TOO_SHORT;

    // 4) fMLLR を推定して、2パス目を SAT モデル（`final.mdl`）で回す
    //    （EVID-ALN-001 の2パス構成）。**推定が進まなければ1パス目のまま。**
    kaldi::Matrix<float> xform;
    if (EstimateFmllr(h->ali_gmm, h->ali_trans, nodes, path, feats, &xform)) {
      kaldi::Matrix<float> adapted(feats.NumRows(), feats.NumCols());
      const int32 dim = feats.NumCols();
      // アフィン変換。**最後の列がオフセット。**
      kaldi::SubMatrix<float> a(xform, 0, dim, 0, dim);
      for (int32 t = 0; t < feats.NumRows(); ++t) {
        kaldi::SubVector<float> dst(adapted, t);
        dst.CopyColFromMat(xform, dim);        // オフセット
        dst.AddMatVec(1.0f, a, kaldi::kNoTrans, feats.Row(t), 1.0f);
      }

      // **グラフは SAT 側の遷移モデルで組み直す。** pdf の割り当てが違う。
      std::vector<Node> sat_nodes;
      if (BuildGraph(h->trans_model, h->ctx_dep, phones, &sat_nodes) &&
          sat_nodes.size() == nodes.size()) {
        std::vector<float> sat_emit;
        Emissions(h->am_gmm, sat_nodes, adapted, &sat_emit);
        std::vector<int32> sat_path;
        const float sat_best = Viterbi(sat_nodes, sat_emit, T, &sat_path);
        // **2パス目が終端へ届いたときだけ採る。** 届かなければ1パス目を残す。
        if (sat_best != -std::numeric_limits<float>::infinity()) {
          nodes = sat_nodes;
          emit = sat_emit;
          path = sat_path;
          best = sat_best;
        }
      }
    }
    if (log_likelihood != nullptr) *log_likelihood = best;

    // 6) 音素の境目をミリ秒で出す。**フレームの先頭を境界にする。**
    //    サブフレーム補間（TR-ALN-06）は Rust 側が事後確率から行う。
    const int32 slots = static_cast<int32>(phones.size());
    for (int32 s = 0; s <= slots; ++s) boundaries_ms[s] = -1.0f;
    boundaries_ms[0] = 0.0f;
    for (int32 t = 0; t < T; ++t) {
      const int32 slot = nodes[path[t]].phone_slot;
      if (boundaries_ms[slot] < 0.0f) {
        boundaries_ms[slot] = static_cast<float>(t) * kFrameShiftMs;
      }
    }
    // 通らなかった音素は、直前の境界と同じ位置（長さ 0）にする。
    for (int32 s = 1; s < slots; ++s) {
      if (boundaries_ms[s] < 0.0f) boundaries_ms[s] = boundaries_ms[s - 1];
    }
    boundaries_ms[slots] = static_cast<float>(T) * kFrameShiftMs;

    // 7) 事後確率（TR-ALN-03）。**前向き後ろ向きで、音素ごとに畳む。**
    //    **採ったパスのグラフと出力尤度で計算する**（2パス目が通れば SAT 側）。
    constexpr float kNegInf = -std::numeric_limits<float>::infinity();
    // **2パス目が通っていれば `nodes` は SAT 側へ差し替わっている。**
    const int32 N = static_cast<int32>(nodes.size());
    if (posteriors != nullptr) {
      if (posteriors_capacity < T * slots) return KOERU_ERR_ARGS;
      std::vector<float> alpha(static_cast<size_t>(T) * N, kNegInf);
      std::vector<float> beta(static_cast<size_t>(T) * N, kNegInf);
      alpha[0] = emit[0];
      for (int32 t = 1; t < T; ++t) {
        for (int32 i = 0; i < N; ++i) {
          const float a = alpha[static_cast<size_t>(t - 1) * N + i];
          if (a == kNegInf) continue;
          for (const auto& arc : nodes[i].arcs) {
            float& dst = alpha[static_cast<size_t>(t) * N + arc.first];
            dst = LogAdd(dst, a + arc.second);
          }
        }
        for (int32 i = 0; i < N; ++i) {
          float& v = alpha[static_cast<size_t>(t) * N + i];
          if (v != kNegInf) v += emit[static_cast<size_t>(t) * N + i];
        }
      }
      beta[static_cast<size_t>(T - 1) * N + (N - 1)] = 0.0f;
      for (int32 t = T - 1; t > 0; --t) {
        for (int32 i = 0; i < N; ++i) {
          float acc = kNegInf;
          for (const auto& arc : nodes[i].arcs) {
            const float b = beta[static_cast<size_t>(t) * N + arc.first];
            if (b == kNegInf) continue;
            acc = LogAdd(acc, arc.second + emit[static_cast<size_t>(t) * N + arc.first] + b);
          }
          beta[static_cast<size_t>(t - 1) * N + i] = acc;
        }
      }
      const float total = alpha[static_cast<size_t>(T - 1) * N + (N - 1)];
      for (int32 t = 0; t < T; ++t) {
        std::vector<float> acc(slots, kNegInf);
        for (int32 i = 0; i < N; ++i) {
          const float a = alpha[static_cast<size_t>(t) * N + i];
          const float b = beta[static_cast<size_t>(t) * N + i];
          if (a == kNegInf || b == kNegInf) continue;
          float& dst = acc[nodes[i].phone_slot];
          dst = LogAdd(dst, a + b - total);
        }
        for (int32 s = 0; s < slots; ++s) {
          posteriors[static_cast<size_t>(t) * slots + s] =
              acc[s] == kNegInf ? 0.0f : std::exp(acc[s]);
        }
      }
    }
    return KOERU_OK;
  } catch (...) {
    return KOERU_ERR_INTERNAL;
  }
}

}  // extern "C"
