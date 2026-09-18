import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked } from "storybook/test";

import { PackageForm } from ".";
import { api } from "~/lib/ipc";
import type { PackageSettingsView, PackageStateView } from "~/lib/ipc";

const empty: PackageSettingsView = {
  distribution_name: "koeru",
  profile: "both",
  author: null,
  voice: null,
  sample: null,
  web: null,
  version: null,
  has_icon: false,
  has_portrait: false,
  portrait_opacity: 1,
  portrait_height: 0,
  tone_range_note: null,
  terms: null,
  credit_example: null,
  contact: null,
  disclaimer: null,
  character_note: null,
};

const state: PackageStateView = {
  may_export: true,
  profile: "both",
  available_profiles: ["classic", "openutau", "both"],
  file_count: 210,
  alias_count: 102,
  exportable_methods: ["single"],
  missing_aliases: [],
  required_table_known: true,
  findings: [],
  unencodable: [],
};

const meta = {
  title: "領域/PackageForm",
  component: PackageForm,
  args: { voiceId: "11111111-1111-4111-8111-111111111111" },
  beforeEach: () => {
    mocked(api.packageSettings).mockResolvedValue(empty);
    mocked(api.packageState).mockResolvedValue(state);
    mocked(api.packageIcon).mockResolvedValue(null);
    mocked(api.packagePortrait).mockResolvedValue(null);
  },
  decorators: [(Story) => <div className="flex w-96 flex-col gap-5">{Story()}</div>],
} satisfies Meta<typeof PackageForm>;

export default meta;
type Story = StoryObj<typeof meta>;

export const まだ何も書いていない: Story = {
  play: async ({ canvasElement }) => {
    // 直していないうちは押させない。押すと同じものを書き直すだけ。
    const save = [...canvasElement.querySelectorAll("button")].find(
      (b) => b.textContent === "書いたことを覚えさせる",
    );
    await expect(save?.hasAttribute("disabled")).toBe(true);
  },
};

export const 書いてある: Story = {
  beforeEach: () => {
    mocked(api.packageSettings).mockResolvedValue({
      ...empty,
      distribution_name: "koeru-mina",
      author: "しお",
      version: "1.0",
      terms: "商用利用もできます。クレジットは任意です。",
      contact: "@example",
    });
  },
};

export const 書き出し方が減っている: Story = {
  beforeEach: () => {
    // CP932 が使えない環境では、classic 互換と両対応が消える（`TR-PKG-13`）。
    mocked(api.packageState).mockResolvedValue({
      ...state,
      profile: "openutau",
      available_profiles: ["openutau"],
    });
    mocked(api.packageSettings).mockResolvedValue({ ...empty, profile: "openutau" });
  },
};
