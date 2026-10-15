import type { ComponentPropsWithRef } from 'react';
export function Button({
  variant = 'primary',
  size = 'standard',
  busy = false,
  disabled,
  className = '',
  children,
  type = 'button',
  ...props
}: ComponentPropsWithRef<'button'> & {
  variant?: 'primary' | 'secondary' | 'danger' | 'quiet';
  size?: 'standard' | 'compact' | 'icon';
  busy?: boolean;
}) {
  return (
    <button
      type={type}
      className={`button ${variant} button-${size} ${className}`}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
      {...props}
    >
      {children}
      {busy ? <span className="caption"> · Please wait…</span> : null}
    </button>
  );
}
