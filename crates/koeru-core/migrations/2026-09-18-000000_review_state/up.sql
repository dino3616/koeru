-- 確認キューの状態を持つ（TR-ALN-25〜27、TR-ALN-30、INV-ALN-001〜004）。
--
-- `align-review.fsl` の状態機械を、再起動をまたいで持ち続けるための席。
-- キューそのものは koeru-align が持っていて、ここはその写しを置く場所になる。

-- ## エントリの状態と、値ごとの固定
--
-- `confirmed` では足りない。 状態は4つあり（未推定・自動確定・確認待ち・書き出し阻止）、
-- 真偽値では「検証で直せない違反があって止まっている」を表せない。
--
-- **固定は値単位で持つ（TR-ALN-30）。** `hand_edited` は「このエントリを人が触った」
-- しか言えないので、「オフセットだけ直して、残りは自動のまま」が表せない。
-- 表せないと、再推定が人の直した値を巻き戻すか、自動の値まで凍るかのどちらかになる。
ALTER TABLE oto_values ADD COLUMN state TEXT NOT NULL DEFAULT 'in_queue';
ALTER TABLE oto_values ADD COLUMN pinned_offset INTEGER NOT NULL DEFAULT 0;
ALTER TABLE oto_values ADD COLUMN pinned_consonant INTEGER NOT NULL DEFAULT 0;
ALTER TABLE oto_values ADD COLUMN pinned_cutoff INTEGER NOT NULL DEFAULT 0;
ALTER TABLE oto_values ADD COLUMN pinned_preutterance INTEGER NOT NULL DEFAULT 0;
ALTER TABLE oto_values ADD COLUMN pinned_overlap INTEGER NOT NULL DEFAULT 0;

-- ## 確信度の成分（TR-ALN-24）
--
-- 「成分ごとの値も保持する」と条文が定めている。 合成スコアだけを持つと、
-- **開き直したあとに TR-ALN-26 (3) の主因ラベルが出せない。**
-- 合成から成分を作り直すこともできない——積で畳んであるので、
-- 同じ値を3つ置くとスコアが3乗になり、主因も常に同じ成分を指す。
--
-- NULL を許す。 この列より前に録ったエントリは成分を持たない。
-- 0 で埋めない——0 は「測ってその値だった」であって「持っていない」ではない。
-- `conf_path` だけは、ほかが埋まっていても NULL がありうる
-- （退避経路は経路確信度を出せない。DEC-ALN-006）。
ALTER TABLE oto_values ADD COLUMN conf_path REAL;
ALTER TABLE oto_values ADD COLUMN conf_sharpness REAL;
ALTER TABLE oto_values ADD COLUMN conf_prior REAL;
ALTER TABLE oto_values ADD COLUMN conf_acoustic REAL;

-- 既にある行を状態へ写す。
--
-- 確認済みでないものは確認待ちへ入れる。 これらは誰にも見られていない推定値で、
-- 自動確定として通すと INV-ALN-003（確認が残っているうちは書き出せない）を
-- 素通りする。**開いた瞬間にキューが伸びるが、それが実際の状態。**
UPDATE oto_values SET state = 'auto_confirmed' WHERE confirmed <> 0;

-- 人が触ったことになっている行は、5値すべてを固定として引き継ぐ。
-- どの値を直したかは記録されていないので、狭めるほうへ倒さない
-- ——固定し過ぎは「自動に戻す」で解けるが、解けた固定は取り戻せない。
UPDATE oto_values
   SET pinned_offset = 1, pinned_consonant = 1, pinned_cutoff = 1,
       pinned_preutterance = 1, pinned_overlap = 1
 WHERE hand_edited <> 0;

-- ## プロジェクトごとの確認の進み方
--
-- 1行しか持たない。 モードは音源ごとに1つで、エントリには付かない
-- （REQ-ALN-010 が切り替えをキュー全体の遷移として定めている）。
CREATE TABLE review_state (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    -- 'individual' | 'batch' | 'suggest_rerecord'
    mode        TEXT    NOT NULL DEFAULT 'individual',
    -- 個別確認をやめたか。 やめられるのは上限を超えたときだけ（INV-ALN-004）で、
    -- 一度きり。超えた事実を持たないと、戻ってまた切り替えられてしまう。
    over_budget INTEGER NOT NULL DEFAULT 0,
    exported    INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO review_state (id) VALUES (1);

-- ## 推定を作った入力の指紋（TR-ALN-29）
--
-- 4つの入力を1つの鍵に畳む。 どれかが変わったときだけ再計算する。
-- モデルが変わったときは自動で上書きしない——アプリを更新したら、
-- 昨日確認し終えた oto が全部作り直されている、は事故。
CREATE TABLE take_fingerprints (
    take_id INTEGER PRIMARY KEY REFERENCES takes(id) ON DELETE CASCADE,
    -- WAV の内容ハッシュ。
    audio   TEXT NOT NULL,
    -- 読み。録音リストがその行に持っている音素列の元。
    reading TEXT NOT NULL,
    -- 規約プリセットの識別（Preset::identity）。
    preset  TEXT NOT NULL,
    -- アライナの識別。モデルの版を含む。
    aligner TEXT NOT NULL
) STRICT;
