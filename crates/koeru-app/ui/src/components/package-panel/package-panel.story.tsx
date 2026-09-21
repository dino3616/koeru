import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked } from "storybook/test";

import { PackagePanel } from ".";
import { api } from "~/lib/ipc";
import type { PackageStateView, PreflightView, RowTakesView } from "~/lib/ipc";

const rows: RowTakesView[] = [
  {
    row_id: "s002",
    text: "か き く け こ",
    state: "recorded",
    units: 5,
    moras: 5,
    risk_hard: 0,
    takes: [
      {
        take_id: 1,
        generation: 1,
        peak: 0.71,
        duration_ms: 3100,
        invalid: false,
        recorded_at: "2026-09-21T18:24:00Z",
      },
    ],
    adopted: 1,
  },
];

const clean: PreflightView = {
  renamed_to_nfc: 0,
  non_nfc_names: [],
  clipped_takes: [],
  may_export: true,
};

const ready: PackageStateView = {
  may_export: true,
  profile: "both",
  available_profiles: ["classic", "openutau", "both"],
  file_count: 12,
  alias_count: 5,
  exportable_methods: ["single"],
  missing_aliases: [],
  required_table_known: true,
  otos_ready: true,
  findings: [],
  unencodable: [],
  downgrades: [],
};

const meta = {
  title: "領域/PackagePanel",
  component: PackagePanel,
  args: { voiceId: "11111111-1111-4111-8111-111111111111", rows, onOpenRow: fn() },
  beforeEach: () => {
    mocked(api.preflight).mockResolvedValue(clean);
    mocked(api.packageState).mockResolvedValue(ready);
  },
  decorators: [(Story) => <div className="flex w-96 flex-col gap-5">{Story()}</div>],
} satisfies Meta<typeof PackagePanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 引っかかるものが無い: Story = {
  play: async ({ canvasElement }) => {
    // 配ることを必須にしない（`TR-PKG-35`）。
    const text = canvasElement.textContent ?? "";
    for (const word of ["公開", "作者", "規約"]) {
      await expect(text).not.toContain(word);
    }
  },
};

export const 割れたテイクがある: Story = {
  beforeEach: () => {
    mocked(api.preflight).mockResolvedValue({
      ...clean,
      renamed_to_nfc: 2,
      clipped_takes: [["s002", 4]],
    });
  },
  play: async ({ canvasElement }) => {
    // 行 ID ではなく読み上げ文字列で指す（`TR-REC-18`）。
    await expect(canvasElement.textContent).not.toContain("s002");
    await expect(canvasElement.textContent).toContain("か き く け こ");
  },
};

export const 書き出せない名前がある: Story = {
  beforeEach: () => {
    mocked(api.preflight).mockResolvedValue({
      ...clean,
      non_nfc_names: ["が"],
      may_export: false,
    });
  },
};

export const 検証で止まっている: Story = {
  beforeEach: () => {
    mocked(api.packageState).mockResolvedValue({
      ...ready,
      may_export: false,
      findings: [
        {
          file: "s002.wav",
          alias: "か",
          row_id: "s002",
          kind: "package.duplicate_alias",
          detail: null,
        },
        {
          file: "s002.wav",
          alias: null,
          row_id: "s002",
          kind: "package.wrong_sample_rate",
          detail: "22050 Hz",
        },
      ],
    });
  },
  play: async ({ canvasElement }) => {
    // どこを直せばよいかまで出す（`TR-PKG-51`）。種別だけで終わらせない。
    await expect(canvasElement.textContent).toContain("同じ呼び名が2つあります");
    await expect(canvasElement.textContent).toContain("22050 Hz");
    await expect(canvasElement.querySelectorAll("button").length).toBeGreaterThan(0);
  },
};

export const 録りきっていない: Story = {
  beforeEach: () => {
    mocked(api.packageState).mockResolvedValue({
      ...ready,
      may_export: false,
      missing_aliases: ["き", "く", "け"],
    });
  },
  play: async ({ canvasElement }) => {
    // 全件並べる（`TR-PKG-23`）。数だけでは何を録れば済むのか分からない。
    await expect(canvasElement.textContent).toContain("まだ録れていない音が");
    await expect(canvasElement.textContent).toContain("き、く、け");
  },
};

export const 見ておく音が残っている: Story = {
  beforeEach: () => {
    mocked(api.packageState).mockResolvedValue({
      ...ready,
      may_export: false,
      otos_ready: false,
    });
  },
  play: async ({ canvasElement }) => {
    // 出せない理由を必ず1つは出す（数えていない関門があると無言で止まる）。
    await expect(canvasElement.textContent).toContain("見ておく音が残っています");
    await expect(canvasElement.textContent).not.toContain("引っかかるものはありません");
  },
};

export const 必要な音の表が無い: Story = {
  beforeEach: () => {
    mocked(api.packageState).mockResolvedValue({
      ...ready,
      may_export: false,
      required_table_known: false,
    });
  },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("必要な音の一覧を、まだ持っていません");
  },
};

export const 書けない文字がある: Story = {
  beforeEach: () => {
    mocked(api.packageState).mockResolvedValue({
      ...ready,
      may_export: false,
      unencodable: [
        {
          place: "voice_name",
          target: null,
          chars: ["🎤"],
          suggestion: "こえる",
          row_id: null,
        },
        {
          place: "alias",
          target: "か",
          chars: ["🎤"],
          suggestion: null,
          row_id: "s002",
        },
      ],
    });
  },
  play: async ({ canvasElement }) => {
    // 置き換えず、代替案を出す（`TR-PKG-17`）。
    await expect(canvasElement.textContent).toContain("こえる");
  },
};
