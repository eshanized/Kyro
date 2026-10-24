import React from "react";

export interface TextFieldProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  helperText?: string;
  error?: string;
  leftIcon?: React.ReactNode;
  rightIcon?: React.ReactNode;
}

export const TextField = React.forwardRef<HTMLInputElement, TextFieldProps>(
  (
    {
      label,
      helperText,
      error,
      leftIcon,
      rightIcon,
      className = "",
      id,
      disabled,
      ...props
    },
    ref
  ) => {
    const inputId = id || React.useId();

    return (
      <div className="space-y-1.5 w-full">
        {label && (
          <label htmlFor={inputId} className="block text-xs font-medium text-neutral-300">
            {label}
          </label>
        )}
        <div className="relative flex items-center">
          {leftIcon && (
            <div className="absolute left-3 text-neutral-500 pointer-events-none flex items-center">
              {leftIcon}
            </div>
          )}
          <input
            id={inputId}
            ref={ref}
            disabled={disabled}
            className={`w-full bg-[#0a0a0a] border rounded-xl text-xs text-neutral-100 placeholder-neutral-500 transition focus:outline-none focus:ring-2 focus:ring-neutral-400 focus:ring-offset-1 focus:ring-offset-[#0a0a0a] disabled:opacity-50 disabled:cursor-not-allowed ${
              leftIcon ? "pl-9" : "pl-3.5"
            } ${rightIcon ? "pr-9" : "pr-3.5"} py-2.5 ${
              error
                ? "border-red-600 focus:ring-red-500"
                : "border-neutral-800 focus:border-neutral-700"
            } ${className}`}
            {...props}
          />
          {rightIcon && (
            <div className="absolute right-3 text-neutral-500 flex items-center">
              {rightIcon}
            </div>
          )}
        </div>
        {error ? (
          <p className="text-[11px] text-red-400 leading-snug">{error}</p>
        ) : helperText ? (
          <p className="text-[11px] text-neutral-500 leading-snug">{helperText}</p>
        ) : null}
      </div>
    );
  }
);
TextField.displayName = "TextField";
