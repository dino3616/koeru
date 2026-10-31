import { createFileRoute } from "@tanstack/react-router";

import { DraftScreen } from "~/screens/draft-screen";

export const Route = createFileRoute("/_studio/voicebank")({
  component: DraftScreen,
});
