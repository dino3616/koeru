import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { Suspense } from "react";

import { CardSkeleton } from "~/components/card-skeleton";
import { PrivacyPanel } from "~/components/privacy-panel";
import { SettingsTabs } from "~/components/settings-tabs";
import {
  api,
  errorMessage,
  type ChoosePrivacyConsentInput,
  type ConsentState,
  type PrivacySettings,
} from "~/lib/ipc";
import { privacyQuery } from "~/lib/queries";
import { SCREEN_NAMES } from "~/lib/screen-names";
import { SETTINGS_TABS, isSettingsTab, type SettingsTab } from "~/lib/settings-tabs";
import { useScreenFocus } from "~/lib/use-screen-focus";

/**
 * アプリの設定。 左にタブの列、右に選んだタブの中身。
 *
 * 選んだタブは検索引数 `tab` が持つ。 state に写さない——写すと、URL と
 * 見えているものが食い違い、収録の準備（`/settings?tab=recording`）へ
 * 直接来た人が一般を見る。 検索引数は経路が検証して既定を足すが、
 * ここは経路を `strict: false` で読むので、狭め直してから使う。
 *
 * `open_project` を呼ばない。 音源は外枠の `ProjectGate` が開く。 設定は
 * 音源を選ぶ前でも開けるので、`id` が無くても中身を出す。
 *
 * `h1` は設定そのもの。 タブの名前の見出しは、各タブの中身が `h2` で持つ。
 * 画面の見た目に「設定」の字は無いので、読み上げにだけ出す。
 */
export const SettingsScreen = () => {
  const heading = useScreenFocus();
  const navigate = useNavigate();
  const { tab } = useSearch({ strict: false });
  const selected: SettingsTab = isSettingsTab(tab) ? tab : "general";

  return (
    <main className="h-full p-4">
      <h1 ref={heading} tabIndex={-1} className="sr-only outline-none">
        {SCREEN_NAMES["/settings"]}
      </h1>
      <SettingsTabs
        selected={selected}
        onSelect={(next) =>
          // 履歴に積まない。 矢印で端から端まで動くだけで、戻る先が6つ増える。
          navigate({ to: "/settings", search: (prev) => ({ ...prev, tab: next }), replace: true })
        }
      >
        {selected === "privacy" ? (
          <Suspense fallback={<CardSkeleton title="プライバシー" />}>
            <PrivacyTab />
          </Suspense>
        ) : (
          <NotYet tab={selected} />
        )}
      </SettingsTabs>
    </main>
  );
};

/** 中身を作る前のタブ。 名前だけを出す。 */
const NotYet = ({ tab }: { tab: SettingsTab }) => (
  <div className="flex flex-col gap-3">
    <h2 className="text-xl font-semibold text-slate-12">
      {SETTINGS_TABS.find((t) => t.id === tab)?.label}
    </h2>
    <p className="text-sm text-slate-11">まだありません</p>
  </div>
);

/**
 * 同意の読みと、切り替えの保存。
 *
 * 保存して初めて切り替わった見た目にする。 先に動かすと、保存できなかった
 * ときに「送ってよい」と答えていない人のスイッチが入ったままに見える——
 * それは、送らないのに送ると言っている画面になる。 返事を待つあいだだけ、
 * 押した向きを出す。
 *
 * 保存できなかったことは値で返る（`StorageProblem`）。 失敗の面へ飛ばさず、
 * スイッチは元のまま、理由をその場に出す。
 */
const PrivacyTab = () => {
  const queryClient = useQueryClient();
  const { data: settings } = useSuspenseQuery(privacyQuery());

  const choose = useMutation({
    mutationFn: (input: ChoosePrivacyConsentInput) => api.choosePrivacyConsent(input),
    onSuccess: ({ outcome }) => {
      if (outcome.__typename === "PrivacyConsentChosen") {
        queryClient.setQueryData(privacyQuery().queryKey, outcome.privacy);
      }
    },
  });

  const asked = choose.variables;
  const answer: ConsentState = asked?.granted === true ? "GRANTED" : "REVOKED";
  const shown: PrivacySettings =
    !choose.isPending || asked === undefined
      ? settings
      : asked.subject === "USAGE"
        ? { ...settings, usage: answer }
        : { ...settings, crashReports: answer };

  const problem =
    choose.error !== null
      ? errorMessage(choose.error)
      : choose.data?.outcome.__typename === "StorageProblem"
        ? choose.data.outcome.message
        : null;

  return (
    <PrivacyPanel
      settings={shown}
      problem={problem}
      onChoose={(subject, granted) => {
        // 返事を待っているあいだの押し直しは受けない。 2つ同時に走ると、
        // どちらの返事が最後に着くかで、見える状態が保存された状態と食い違う。
        if (choose.isPending) return;
        choose.mutate({ subject, granted });
      }}
    />
  );
};
