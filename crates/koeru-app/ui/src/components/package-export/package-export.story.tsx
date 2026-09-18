import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked } from "storybook/test";

import { PackageExport } from ".";
import { api } from "~/lib/ipc";
import type { PackageStateView } from "~/lib/ipc";

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
  names_ready: true,
  findings: [],
  unencodable: [],
};

const meta = {
  title: "領域/PackageExport",
  component: PackageExport,
  args: { voiceId: "11111111-1111-4111-8111-111111111111" },
  beforeEach: () => {
    mocked(api.packageState).mockResolvedValue(ready);
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
