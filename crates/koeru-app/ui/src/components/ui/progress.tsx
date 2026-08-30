import * as ProgressPrimitive from "@radix-ui/react-progress";
import type { ComponentProps } from "react";

import { cn } from "~/lib/cn";

/**
 * 進み具合。
 *
 * **分母に書き出し・公開・作者情報を含めない**（TR-PKG-35）。
 * ここに渡してよいのは録音カバレッジだけ。
 */
export const Progress = ({
  className,
  value,
  ...props
}: ComponentProps<typeof ProgressPrimitive.Root>) => (
  <ProgressPrimitive.Root
    className={cn("relative h-2 w-full overflow-hidden rounded-full bg-surface-2", className)}
    value={value}
    {...props}
  >
    <ProgressPrimitive.Indicator
      className="h-full rounded-full bg-accent transition-[width]"
      style={{ width: `${value ?? 0}%` }}
    />
  </ProgressPrimitive.Root>
);
