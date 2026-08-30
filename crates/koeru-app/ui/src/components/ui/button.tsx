import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "~/lib/cn";

/**
 * 押せるもの。
 *
 * **高さは 44px 以上を既定にする**（TR-PLT-28 の対象サイズ）。
 * 収録中は画面を見ずに押すことがあるので、小さい的にしない。
 *
 * **キーボードだけで到達でき、閉じ込められない**（TR-PLT-26）。
 * 素の `<button>` のまま出すので、Tab の順序も Enter / Space も既定のまま効く。
 */
const buttonVariants = cva(
  cn(
    "inline-flex items-center justify-center gap-2 rounded-lg font-medium",
    "transition-colors disabled:pointer-events-none disabled:opacity-45",
    "focus-visible:outline-2 focus-visible:outline-offset-2",
  ),
  {
    variants: {
      variant: {
        primary: "bg-accent text-accent-ink hover:brightness-110",
        secondary: "bg-surface-2 text-text hover:bg-surface-3",
        ghost: "text-text-dim hover:bg-surface-2 hover:text-text",
        danger: "bg-danger text-danger-ink hover:brightness-110",
      },
      size: {
        md: "h-11 px-4 text-sm",
        lg: "h-14 px-6 text-base",
        icon: "size-11",
      },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

type ButtonProps = ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    /** 別の要素として描く（リンクなど）。 */
    asChild?: boolean;
  };

export const Button = ({ className, variant, size, asChild = false, ...props }: ButtonProps) => {
  const Comp = asChild ? Slot : "button";
  return <Comp className={cn(buttonVariants({ variant, size }), className)} {...props} />;
};

export { buttonVariants };
