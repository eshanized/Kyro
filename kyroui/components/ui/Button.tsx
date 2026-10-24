import React from "react";
import { Loader2 } from "lucide-react";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "outline" | "ghost" | "danger";
  size?: "xs" | "sm" | "md" | "lg";
  isLoading?: boolean;
  leftIcon?: React.ReactNode;
  rightIcon?: React.ReactNode;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      children,
      variant = "primary",
      size = "md",
      isLoading = false,
      leftIcon,
      rightIcon,
      className = "",
      disabled,
      ...props
    },
    ref
  ) => {
    const baseStyles =
      "inline-flex items-center justify-center font-medium rounded-xl transition select-none disabled:opacity-50 disabled:cursor-not-allowed focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-neutral-400 focus-visible:ring-offset-1 focus-visible:ring-offset-[#0a0a0a]";

    const sizeStyles = {
      xs: "text-[11px] px-2.5 py-1 gap-1.5 h-7",
      sm: "text-xs px-3 py-1.5 gap-1.5 h-8",
      md: "text-xs px-4 py-2 gap-2 h-9",
      lg: "text-sm px-5 py-2.5 gap-2.5 h-11",
    }[size];

    const variantStyles = {
      primary: "bg-[#ededed] hover:bg-white text-[#0a0a0a] font-semibold shadow-sm",
      secondary: "bg-[#1f1f1f] hover:bg-[#262626] text-neutral-200 border border-neutral-700/80 shadow-sm",
      outline: "bg-transparent hover:bg-neutral-800/60 text-neutral-200 border border-neutral-700/80",
      ghost: "bg-transparent hover:bg-neutral-800/60 text-neutral-400 hover:text-neutral-100",
      danger: "bg-red-600/20 hover:bg-red-600/30 text-red-400 border border-red-500/30",
    }[variant];

    return (
      <button
        ref={ref}
        disabled={disabled || isLoading}
        className={`${baseStyles} ${sizeStyles} ${variantStyles} ${className}`}
        {...props}
      >
        {isLoading ? (
          <Loader2 className="w-3.5 h-3.5 animate-spin shrink-0" />
        ) : (
          leftIcon && <span className="shrink-0">{leftIcon}</span>
        )}
        <span>{children}</span>
        {!isLoading && rightIcon && <span className="shrink-0">{rightIcon}</span>}
      </button>
    );
  }
);
Button.displayName = "Button";

export interface IconButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  "aria-label": string;
  variant?: "primary" | "secondary" | "outline" | "ghost" | "danger";
  size?: "xs" | "sm" | "md" | "lg";
  isLoading?: boolean;
}

export const IconButton = React.forwardRef<HTMLButtonElement, IconButtonProps>(
  (
    {
      children,
      variant = "ghost",
      size = "md",
      isLoading = false,
      className = "",
      disabled,
      "aria-label": ariaLabel,
      ...props
    },
    ref
  ) => {
    const baseStyles =
      "inline-flex items-center justify-center rounded-xl transition select-none disabled:opacity-50 disabled:cursor-not-allowed focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-neutral-400 focus-visible:ring-offset-1 focus-visible:ring-offset-[#0a0a0a]";

    const sizeStyles = {
      xs: "w-7 h-7 text-xs",
      sm: "w-8 h-8 text-xs",
      md: "w-9 h-9 text-sm",
      lg: "w-11 h-11 text-base",
    }[size];

    const variantStyles = {
      primary: "bg-[#ededed] hover:bg-white text-[#0a0a0a] shadow-sm",
      secondary: "bg-[#1f1f1f] hover:bg-[#262626] text-neutral-200 border border-neutral-700/80 shadow-sm",
      outline: "bg-transparent hover:bg-neutral-800/60 text-neutral-300 border border-neutral-700/80",
      ghost: "bg-transparent hover:bg-neutral-800/60 text-neutral-400 hover:text-neutral-100",
      danger: "bg-red-600/20 hover:bg-red-600/30 text-red-400 border border-red-500/30",
    }[variant];

    return (
      <button
        ref={ref}
        aria-label={ariaLabel}
        disabled={disabled || isLoading}
        className={`${baseStyles} ${sizeStyles} ${variantStyles} ${className}`}
        {...props}
      >
        {isLoading ? <Loader2 className="w-3.5 h-3.5 animate-spin shrink-0" /> : children}
      </button>
    );
  }
);
IconButton.displayName = "IconButton";
