import { createFileRoute } from "@tanstack/react-router";

import { DraftScreen } from "~/screens/draft-screen";

/** 新しい音源を作る画面。 まだ音源が無いので、`id` を渡さずに来る。 */
export const Route = createFileRoute("/_studio/new")({
  component: DraftScreen,
});
