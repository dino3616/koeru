import type { ComponentProps } from "react";

import { cn } from "~/lib/cn";

export const Card = ({ className, ...props }: ComponentProps<"section">) => (
  <section className={cn("rounded-xl border border-border bg-surface p-5", className)} {...props} />
);

export const CardTitle = ({ className, ...props }: ComponentProps<"h2">) => (
  <h2 className={cn("text-sm font-semibold tracking-wide text-text-dim", className)} {...props} />
);
