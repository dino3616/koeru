import { createFileRoute } from "@tanstack/react-router";

import { LibraryScreen } from "~/screens/library-screen";

export const Route = createFileRoute("/")({
  component: LibraryScreen,
});
