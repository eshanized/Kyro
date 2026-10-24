import React from "react";

export interface SurfaceProps extends React.HTMLAttributes<HTMLDivElement> {
  variant?: "default" | "elevated" | "sunken" | "panel";
  interactive?: boolean;
}

export const Surface = React.forwardRef<HTMLDivElement, SurfaceProps>(
  ({ children, variant = "default", interactive = false, className = "", ...props }, ref) => {
    const variantStyles = {
      default: "bg-[#141414] border border-neutral-800/80 text-neutral-200",
      elevated: "bg-[#1a1a1a] border border-neutral-700/80 shadow-lg text-neutral-200",
      sunken: "bg-[#0a0a0a] border border-neutral-900 text-neutral-200",
      panel: "bg-[#111111] border border-neutral-800 text-neutral-200",
    }[variant];

    const interactiveStyles = interactive
      ? "hover:border-neutral-700 hover:bg-[#1a1a1a] transition cursor-pointer"
      : "";

    return (
      <div
        ref={ref}
        className={`rounded-2xl ${variantStyles} ${interactiveStyles} ${className}`}
        {...props}
      >
        {children}
      </div>
    );
  }
);
Surface.displayName = "Surface";
