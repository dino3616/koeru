/*
 * Rust の識別子を、画面に出す日本語へ直す。
 *
 * `~/lib/ipc` にも同じ役目のものがある（`micModeLabel`）。 あちらは
 * 境界の層に属するもので、ここは画面の語彙。 分けているのは、
 * ここが `docs/design/direction.md` の言葉の規律に従うため——
 * 内部表現の名前を出さない、評価しない、単位を省かない。
 */

/** 方式の名前。まだ単独音しか作れない（`PROFILE-M5` で増える）。 */
const METHODS: Record<string, string> = {
  single: "単独音",
  vcv: "連続音",
  cvvc: "CVVC",
};

/** 方式。読めなければ「作り方が分かりません」。 */
export const methodLabel = (method: string | null): string =>
  method === null ? "作り方が分かりません" : (METHODS[method] ?? "知らない作り方");

/**
 * 低確信度の主因（`TR-ALN-26` (3)）。
 *
 * 用語を出さない。 要件が「境界が曖昧 / 他のテイクと違う / 音が割れている 等」と
 * 例示しているとおり、成分の名前ではなく起きていることを書く。
 * 上級モードの数値表示は `PROFILE-M6`（`Q-PLT-004` 待ち）。
 */
const CAUSES: Record<string, string> = {
  "confidence.path": "どこで切るか決めきれませんでした",
  "confidence.sharpness": "音の変わり目がはっきりしません",
  "confidence.prior": "ほかの回と切り方が違います",
  "confidence.acoustic": "音が割れているか、小さすぎます",
};

/** 主因。無ければ `null`——「理由なし」と「まだ分からない」を分ける。 */
export const causeLabel = (cause: string | null): string | null =>
  cause === null ? null : (CAUSES[cause] ?? "理由を特定できませんでした");

/** 確認の進め方（`TR-ALN-25`）。 */
const REVIEW_MODES: Record<string, string> = {
  individual: "1件ずつ確認",
  batch: "まとめて確認",
  suggest_rerecord: "録り直しをすすめる",
};

/** 確認の進め方。読めなければ既定の言い方に倒す。 */
export const reviewModeLabel = (mode: string): string =>
  REVIEW_MODES[mode] ?? REVIEW_MODES["individual"] ?? mode;

/**
 * 5値の名前（`TR-ALN-30`）。
 *
 * `take-values` の言い換えと同じ語を使う。 同じ値が画面の場所によって
 * 別の名前で出ると、固定を解く的がどれを指しているのか分からなくなる。
 */
const SLOTS: Record<string, string> = {
  offset: "頭の余白",
  preutterance: "歌い出しの位置",
  overlap: "前の音との重なり",
  consonant: "伸ばさないところ",
  cutoff: "終わりの余白",
};

/** 5値の名前。 */
export const slotLabel = (slot: string): string => SLOTS[slot] ?? slot;

/**
 * 書き出す前の検査で見つかるもの（`TR-PKG-49`）。
 *
 * 種別は Rust が返す固定文字列で、文面はここが持つ。 Rust 側へ文面を
 * 置くと、送信層に出てはいけない値と同じ場所に画面の語彙が混ざる。
 */
const FINDINGS: Record<string, string> = {
  "package.character_txt_missing": "音源の名前が空です",
  "package.character_yaml_missing": "配るものが1つもありません",
  "package.wav_missing": "録った音が見つかりません",
  "package.wav_unreadable": "録った音を読めません",
  "package.empty_wav": "録った音の長さが 0 です",
  "package.wrong_sample_rate": "録った音の細かさが配布用と違います",
  "package.unsupported_format": "この形式の音は配れません",
  "package.malformed_oto_line": "この呼び名は設定ファイルに書けない字を含みます",
  "package.oto_value": "原音設定の値が範囲の外にあります",
  "package.cutoff_before_preutterance": "使う終わりが、発声の始まりより手前にあります",
  "package.cutoff_before_overlap": "使う終わりが、重ねる位置より手前にあります",
  "package.cutoff_not_after_consonant": "使う終わりが、子音の終わりを越えていません",
  "package.cutoff_beyond_file": "使う終わりが、録った音の外にあります",
  "package.duplicate_alias": "同じ呼び名が2つあります",
  "package.alias_shape": "呼び名に、使えない空白か分解された字が入っています",
  "package.nfd_name": "受け取る側で見つからなくなる名前です",
  "package.case_collision": "大文字と小文字だけが違う名前が2つあります",
  "package.case_mismatch": "設定ファイルの参照と、実際の名前の大小が違います",
  "package.frq_too_short": "音の高さの表が、録った音の最後まで届いていません",
  "package.frq_malformed": "音の高さの表が壊れています",
  "package.frq_unvoiced_not_zero": "音の高さの表に、読めない値が入っています",
};

/** 検査で見つかったものの言い方。知らない種別はそのまま出さない。 */
export const findingLabel = (kind: string): string => FINDINGS[kind] ?? "書き出せない状態です";

/**
 * 検査で添えられる細かい理由（`TR-PKG-51`）。
 *
 * 種別だけでは「原音設定の値が範囲の外にあります」で止まる。 どの値が
 * どうおかしいのかまで出さないと、直しに行けない。
 */
const DETAILS: Record<string, string> = {
  "oto.negative_offset": "頭の余白が負になっています",
  "oto.offset_beyond_file": "頭の余白が、録った音より後ろにあります",
  "oto.negative_preutterance": "歌い出しの位置が負になっています",
  "oto.preutterance_beyond_file": "歌い出しの位置が、録った音より後ろにあります",
  "oto.negative_consonant": "伸ばさないところの長さが負になっています",
  "oto.consonant_beyond_file": "伸ばさないところが、録った音より後ろにあります",
  "oto.overlap_beyond_file": "前の音との重なりが、録った音より後ろにあります",
  "oto.empty_region": "使えるところが残っていません",
  "alias.edge_space": "前後に空白があります",
  "alias.double_space": "空白が2つ続いています",
  "alias.ideographic_space": "全角の空白が入っています",
  "alias.not_nfc": "分解された字が入っています",
  "alias.empty": "空です",
};

/**
 * 細かい理由の言い方。
 *
 * 表に無いものはそのまま出す。 レートやフレーム数のように、
 * 値そのものが理由になっているものがある（`22050 Hz` など）。
 */
export const detailLabel = (detail: string | null): string | null =>
  detail === null ? null : (DETAILS[detail] ?? detail);

/** 書き出し方（`TR-PKG-12`）。 */
const PROFILES: Record<string, string> = {
  both: "どちらでも開ける形",
  classic: "UTAU 向け",
  openutau: "OpenUtau 向け",
};

/** 書き出し方の言い方。 */
export const profileLabel = (profile: string): string => PROFILES[profile] ?? profile;

/** 書き出し方が何を変えるか。選ぶときの一行。 */
const PROFILE_HINTS: Record<string, string> = {
  both: "UTAU でも OpenUtau でも開けます。迷ったらこれ。",
  classic: "UTAU 本体向け。OpenUtau では文字が化けることがあります。",
  openutau: "OpenUtau 向け。UTAU 本体では文字が化けることがあります。",
};

/** 書き出し方の一行説明。 */
export const profileHint = (profile: string): string => PROFILE_HINTS[profile] ?? "";

/** CP932 で書けない文字が出ている場所（`TR-PKG-17`）。 */
const PLACES: Record<string, string> = {
  voice_name: "音源の名前",
  character_field: "音源の情報",
  alias: "呼び名",
  readme_section: "説明文",
};

/** 書けない文字が出ている場所の言い方。 */
export const placeLabel = (place: string): string => PLACES[place] ?? "どこか";
