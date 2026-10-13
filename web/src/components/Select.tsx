import { useId, type SelectHTMLAttributes } from 'react';
export function Select({
  label,
  hint,
  error,
  id,
  children,
  ...props
}: SelectHTMLAttributes<HTMLSelectElement> & { label: string; hint?: string; error?: string }) {
  const generated = useId();
  const key = id ?? generated;
  return (
    <div className="field">
      <label htmlFor={key}>{label}</label>
      <select
        id={key}
        aria-invalid={error ? true : undefined}
        aria-describedby={hint || error ? `${key}-help` : undefined}
        {...props}
      >
        {children}
      </select>
      {hint || error ? (
        <p id={`${key}-help`} className={error ? 'error caption' : 'caption muted'}>
          {error ?? hint}
        </p>
      ) : null}
    </div>
  );
}
