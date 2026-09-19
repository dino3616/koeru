import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, waitFor } from "storybook/test";

import { PackageExport } from ".";
import { api } from "~/lib/ipc";
import type { PackageSettingsView, PackageStateView, PreflightView } from "~/lib/ipc";

const settings: PackageSettingsView = {
  distribution_name: "koeru",
  profile: "both",
  author: null,
  voice: null,
  sample: null,
  web: null,
  version: "1.0",
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
  file_count: 210,
  alias_count: 102,
  exportable_methods: ["single"],
  missing_aliases: [],
  required_table_known: true,
  otos_ready: true,
  findings: [],
  unencodable: [],
};

const meta = {
  title: "領域/PackageExport",
  component: PackageExport,
  args: { voiceId: "11111111-1111-4111-8111-111111111111" },
  beforeEach: () => {
    mocked(api.packageState).mockResolvedValue(ready);
    mocked(api.preflight).mockResolvedValue(clean);
    mocked(api.packageSettings).mockResolvedValue(settings);
  },
  decorators: [(Story) => <div className="flex w-96 flex-col gap-5">{Story()}</div>],
} satisfies Meta<typeof PackageExport>;

export default meta;
type Story = StoryObj<typeof meta>;

export const つくれる: Story = {
  play: async ({ canvasElement }) => {
    const button = canvasElement.querySelector("button");
    await expect(button?.hasAttribute("disabled")).toBe(false);
  },
};

export const まだつくれない: Story = {
  beforeEach: () => {
    mocked(api.packageState).mockResolvedValue({ ...ready, may_export: false });
  },
  play: async ({ canvasElement }) => {
    // 検証を通らないまま押せない（`FB-PKG-102`）。
    const button = canvasElement.querySelector("button");
    await expect(button?.hasAttribute("disabled")).toBe(true);
  },
};

export const 名前が直らないと作れない: Story = {
  beforeEach: () => {
    // 名前の関門は `preflight` が持つ（`TR-REC-32`）。数え直さない。
    mocked(api.preflight).mockResolvedValue({
      ...clean,
      non_nfc_names: ["が"],
      may_export: false,
    });
  },
  play: async ({ canvasElement }) => {
    const button = canvasElement.querySelector("button");
    await expect(button?.hasAttribute("disabled")).toBe(true);
  },
};

export const 呼び名を付けていない: Story = {
  beforeEach: () => {
    mocked(api.packageSettings).mockResolvedValue({ ...settings, version: null });
  },
  play: async ({ canvasElement }) => {
    // 札は上の面で決める。ここで打たせない（2箇所あると食い違う）。
    await expect(canvasElement.querySelectorAll("input").length).toBe(0);
    await expect(canvasElement.textContent).toContain("呼び名は付いていません");
  },
};

export const つくったあと: Story = {
  play: async ({ canvasElement, userEvent }) => {
    mocked(api.exportPackage).mockResolvedValue({
      seq: 1,
      archive_name: "000001-v1.0.zip",
      alias_count: 102,
      released_at: "2026-09-19T00:20:00Z",
    });
    const make = [...canvasElement.querySelectorAll("button")].find(
      (b) => b.textContent === "つくる",
    );
    await userEvent.click(make as HTMLButtonElement);
    // 作れるのに手が届かない状態にしない（`TR-PKG-45`）。
    await waitFor(async () => {
      await expect(canvasElement.textContent).toContain("置き場所を開く");
    });
  },
};
