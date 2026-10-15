import { useId, type InputHTMLAttributes, type TextareaHTMLAttributes } from 'react';
type Common = { label: string; hint?: string; error?: string };
export function Field({
  label,
  hint,
  error,
  id,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & Common) {
  const generated = useId();
  const key = id ?? generated;
  return (
    <div className="field">
      <label htmlFor={key}>{label}</label>
      <input
        id={key}
        aria-invalid={error ? true : undefined}
        aria-errormessage={error ? `${key}-help` : undefined}
        aria-describedby={hint || error ? `${key}-help` : undefined}
        {...props}
      />
      {hint || error ? (
        <p id={`${key}-help`} className={error ? 'error caption' : 'caption muted'}>
          {error ?? hint}
        </p>
      ) : null}
    </div>
  );
}
export function TextAreaField({
  label,
  hint,
  error,
  id,
  ...props
}: TextareaHTMLAttributes<HTMLTextAreaElement> & Common) {
  const generated = useId();
  const key = id ?? generated;
  return (
    <div className="field">
      <label htmlFor={key}>{label}</label>
      <textarea
        id={key}
        aria-invalid={error ? true : undefined}
        aria-errormessage={error ? `${key}-help` : undefined}
        aria-describedby={hint || error ? `${key}-help` : undefined}
        {...props}
      />
      {hint || error ? (
        <p id={`${key}-help`} className={error ? 'error caption' : 'caption muted'}>
          {error ?? hint}
        </p>
      ) : null}
    </div>
  );
}
