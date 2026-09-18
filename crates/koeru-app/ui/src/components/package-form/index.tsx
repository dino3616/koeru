import { useMutation, useQueryClient, useSuspenseQueries } from "@tanstack/react-query";
import { useId, useState } from "react";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { Field } from "~/components/field";
import { NoteField } from "~/components/note-field";
import { PackageImage } from "~/components/package-image";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "~/components/select";
import { api, errorMessage, type PackageSettingsView } from "~/lib/ipc";
import { profileHint, profileLabel } from "~/lib/labels";
import { ledgerKey, packageSettingsQuery, packageStateQuery } from "~/lib/queries";

type PackageFormProps = {
  voiceId: string;
};

/**
 * 配り物にのせること（`PROFILE-M4`）。
 *
 * 必須なのは配布名だけ。 ほかは書かなくても配れる（`DEC-PKG-011`）。
 * 書いた節だけが説明書に出るので、空欄のまま進んでよい。
 *
 * 配布名は音源の名前と別（`DEC-PKG-008`）。 音源の名前は日本語も絵文字も
 * 通るが、フォルダの名前にはできない。既定値は名前から作ってあるので、
 * 読める名前にしたい人だけが直す。
 *
 * 打つたびには保存しない。 押したときにまとめて送る——1文字ごとに
 * 台帳を書くと、そのたびに検証が走り直す（WAV を全部開く）。
 */
export const PackageForm = ({ voiceId }: PackageFormProps) => {
  const ids = useIds();
  const queryClient = useQueryClient();
  /*
   * まとめて並行に取る。
   *
   * `useSuspenseQuery` を並べない。 同じ部品に並べると**直列**になる
   * （`EVID-PLT-001` で実測）。書き出せるかの判定は WAV を全部開くので、
   * 直列にすると設定の欄が出るまで待たされる。
   */
  const [{ data: saved }, { data: state }] = useSuspenseQueries({
    queries: [packageSettingsQuery(voiceId), packageStateQuery(voiceId)],
  });
  const [draft, setDraft] = useState<PackageSettingsView>(saved);

  const save = useMutation({
    mutationFn: (next: PackageSettingsView) => api.setPackageSettings(trimmed(next)),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ledgerKey }),
  });

  const set = <K extends keyof PackageSettingsView>(key: K, value: PackageSettingsView[K]) =>
    setDraft((d) => ({ ...d, [key]: value }));
  /** 打った空文字は「未記入」に戻す（`DEC-PKG-011`）。 */
  const text = (key: TextKey) => ({
    value: draft[key] ?? "",
    onChange: (e: { target: { value: string } }) =>
      set(key, e.target.value === "" ? null : e.target.value),
  });

  /*
   * 絵の有無は比べない。
   *
   * 絵は別の口で入れ替える（[`PackageImage`]）ので、入れ替えた時点で
   * `saved` だけが動く。比べると、**何も打っていないのに保存が押せるようになる。**
   */
  const changed = JSON.stringify(editable(draft)) !== JSON.stringify(editable(saved));

  return (
    <>
      <Card title="配るときの形">
        <Field
          id={ids.name}
          label="フォルダの名前"
          hint="英数字とハイフンだけ。受け取った人のパソコンに、この名前で入ります。"
          value={draft.distribution_name}
          onChange={(e) => set("distribution_name", e.target.value)}
        />

        <div className="flex flex-col gap-2">
          <span className="text-xs text-slate-11" id={ids.profileLabel}>
            開ける相手
          </span>
          <Select value={draft.profile} onValueChange={(next) => set("profile", next)}>
            <SelectTrigger aria-labelledby={ids.profileLabel}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {state.available_profiles.map((p) => (
                <SelectItem key={p} value={p}>
                  {profileLabel(p)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <p className="text-xs text-slate-11">{profileHint(draft.profile)}</p>
        </div>

        <PackageImage
          voiceId={voiceId}
          slot="icon"
          label="一覧に出る絵"
          hint="正方形に切って、100×100 にして入れます。PNG か JPEG。"
        />
        <PackageImage
          voiceId={voiceId}
          slot="portrait"
          label="立ち絵"
          hint="OpenUtau でだけ出ます。PNG か JPEG。"
        />
      </Card>

      <Card title="説明書に載せること">
        <p className="text-sm text-slate-12">
          書かなくても配れます。書いた分だけ、説明書に載ります。
        </p>

        <Field id={ids.author} label="作った人の名前" {...text("author")} />
        <Field id={ids.voice} label="声の人の名前" {...text("voice")} />
        <Field id={ids.version} label="この回の呼び名" {...text("version")} />
        <Field id={ids.web} label="置いてある場所" {...text("web")} />
        <Field id={ids.contact} label="連絡先" {...text("contact")} />
        <Field
          id={ids.toneRange}
          label="すすめたい高さ"
          hint="C3 から C5 くらい、のように。"
          {...text("tone_range_note")}
        />

        <NoteField
          id={ids.terms}
          label="使ってよい範囲"
          hint="決まった形はありません。書いたものがそのまま載ります。"
          lines={10}
          {...text("terms")}
        />
        <NoteField id={ids.credit} label="名前の書き方の例" {...text("credit_example")} />
        <NoteField id={ids.character} label="この子のこと" lines={6} {...text("character_note")} />
        <NoteField id={ids.disclaimer} label="ことわり書き" {...text("disclaimer")} />
      </Card>

      <div className="flex flex-col gap-2">
        <Button
          variant="primary"
          onClick={() => save.mutate(draft)}
          disabled={!changed || save.isPending}
        >
          {save.isPending ? "書いています" : "書いたことを覚えさせる"}
        </Button>
        {save.error !== null && (
          <p role="alert" className="text-sm text-red-11">
            {errorMessage(save.error)}
          </p>
        )}
      </div>
    </>
  );
};

/** 空白だけの欄は「未記入」に揃える（`DEC-PKG-011`）。 */
const trimmed = (v: PackageSettingsView): PackageSettingsView => {
  const clean = (s: string | null) => {
    const t = (s ?? "").trim();
    return t === "" ? null : t;
  };
  return {
    ...v,
    distribution_name: v.distribution_name.trim(),
    author: clean(v.author),
    voice: clean(v.voice),
    sample: clean(v.sample),
    web: clean(v.web),
    version: clean(v.version),
    tone_range_note: clean(v.tone_range_note),
    terms: clean(v.terms),
    credit_example: clean(v.credit_example),
    contact: clean(v.contact),
    disclaimer: clean(v.disclaimer),
    character_note: clean(v.character_note),
  };
};

/** この面が書き換える値だけを取り出す。絵の有無は入らない。 */
const editable = (v: PackageSettingsView) => {
  const { has_icon: _icon, has_portrait: _portrait, ...rest } = trimmed(v);
  return rest;
};

/** 文字を打つ欄の鍵。画像と真偽値はここに含めない。 */
type TextKey =
  | "author"
  | "voice"
  | "sample"
  | "web"
  | "version"
  | "tone_range_note"
  | "terms"
  | "credit_example"
  | "contact"
  | "disclaimer"
  | "character_note";

/** 名札と欄を結ぶ識別子。`useId` を欄の数だけ呼ぶ。 */
const useIds = () => ({
  name: useId(),
  profileLabel: useId(),
  author: useId(),
  voice: useId(),
  version: useId(),
  web: useId(),
  contact: useId(),
  toneRange: useId(),
  terms: useId(),
  credit: useId(),
  character: useId(),
  disclaimer: useId(),
});
