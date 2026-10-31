import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { expect, mocked, userEvent, waitFor, within } from "storybook/test";

import { ProjectGate } from ".";
import { api } from "~/lib/ipc";

const ID = "11111111-1111-4111-8111-111111111111";

const 開いた = () => {
  mocked(api.openProject).mockResolvedValue({
    next_row_id: "s003",
    next_row_text: "さ し す せ そ",
    covered: 10,
    required: 102,
    coverage: "InProgress",
    handoff: "NotExported",
    singable_songs: 0,
    songs_in_bank: 4,
    by_tone: [],
    remaining_seconds: 600,
    remaining_rows: 72,
  });
};

const meta = {
  title: "部品/ProjectGate",
  component: ProjectGate,
  args: { id: ID, children: <p>中身</p> },
} satisfies Meta<typeof ProjectGate>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 開いている途中: Story = {};

export const 開いたあと: Story = {
  beforeEach: 開いた,
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("中身"));
  },
};

export const 音源なし: Story = {
  args: { id: undefined },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("中身");
    await expect(api.openProject).not.toHaveBeenCalled();
  },
};

/** 場を移るのと同じ形。 外枠は残り、中身だけが差し替わる。 */
const 場を移る = ({ id }: { id: string }) => {
  const [place, setPlace] = useState<"収録" | "曲バンク">("収録");
  return (
    <ProjectGate id={id}>
      <p>{place}</p>
      <button type="button" onClick={() => setPlace("曲バンク")}>
        曲バンクへ
      </button>
    </ProjectGate>
  );
};

export const 場を移っても開き直さない: Story = {
  beforeEach: 開いた,
  render: () => <場を移る id={ID} />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole("button", { name: "曲バンクへ" }));
    await waitFor(() => expect(canvasElement.textContent).toContain("曲バンク"));
    // 開き直すと、収録セッションが録音の途中で切り替わる。
    await expect(api.openProject).toHaveBeenCalledTimes(1);
  },
};
