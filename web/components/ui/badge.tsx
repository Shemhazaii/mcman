import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "../../lib/utils";

const badgeVariants = cva(
  "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium transition-colors",
  {
    variants: {
      variant: {
        default: "bg-[var(--panel2)] text-[var(--dim)]",
        primary: "bg-[#204a2c] text-[#9fe0af]",
        secondary: "bg-[var(--panel2)] text-[var(--tx)]",
        destructive: "bg-[#4a2521] text-[#f0a89e]",
        warning: "bg-[#4a3a1e] text-[#e8c06a]",
        plugin: "bg-[#3a3320] text-[#e3c98a]",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant }), className)} {...props} />
  );
}

export { Badge, badgeVariants };
