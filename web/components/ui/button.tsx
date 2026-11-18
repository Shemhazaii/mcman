import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "../../lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--acc)] disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      variant: {
        default:
          "border border-[var(--bd)] bg-[var(--panel2)] text-[var(--tx)] hover:border-[var(--acc)]",
        primary:
          "border border-[#2f6b40] bg-[#204a2c] text-[#9fe0af] hover:brightness-110",
        destructive:
          "border border-[var(--bd)] bg-[var(--panel2)] text-[var(--tx)] hover:border-[var(--red)]",
        outline:
          "border border-[var(--bd)] bg-transparent text-[var(--tx)] hover:border-[var(--acc)]",
        accent:
          "border border-[#2b6fb8] bg-[#4da3ff] text-[#0b1520] font-semibold shadow-[0_0_0_2px_rgba(77,163,255,0.25)] hover:brightness-110",
        ghost:
          "border border-transparent bg-transparent text-[var(--tx)] hover:bg-[var(--panel2)]",
      },
      size: {
        default: "h-9 px-4 py-2",
        sm: "h-8 rounded-md px-3 text-xs",
        lg: "h-10 rounded-md px-8",
        icon: "h-9 w-9",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";

export { Button, buttonVariants };
