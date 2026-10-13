import { useId } from 'react';
export function Switch({
  label,
  checked,
  pending = false,
  disabled = false,
  error,
  onChange,
}: {
  label: string;
  checked: boolean;
  pending?: boolean;
  disabled?: boolean;
  error?: string;
  onChange: (checked: boolean) => void;
}) {
  const id = useId();
  return (
    <div className="field">
      <button
        className="switch-control"
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled || pending}
        aria-busy={pending || undefined}
        aria-describedby={pending || error ? `${id}-help` : undefined}
        onClick={() => onChange(!checked)}
      >
        <span className={`switch-track ${checked ? 'on' : ''}`} aria-hidden="true">
          <span />
        </span>
        {label}
      </button>
      {pending || error ? (
        <p id={`${id}-help`} className={error ? 'error caption' : 'caption'}>
          {error ?? 'Saving…'}
        </p>
      ) : null}
    </div>
  );
}
