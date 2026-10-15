import { useEffect, useId, useRef, type ReactNode } from 'react';
import { Button } from './Button.tsx';
export function Dialog({
  open,
  title,
  description,
  onClose,
  children,
  busy = false,
  dirty = false,
  footer,
  className = '',
  cancelLabel = 'Cancel',
}: {
  open: boolean;
  title: string;
  description?: string;
  onClose: () => void;
  children: ReactNode;
  busy?: boolean;
  dirty?: boolean;
  footer?: ReactNode;
  className?: string;
  cancelLabel?: string;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const id = useId();
  const closeRef = useRef(onClose);
  const state = useRef({ busy, dirty });
  const lastError = useRef('');
  useEffect(() => {
    closeRef.current = onClose;
    state.current = { busy, dirty };
  });
  const close = () => {
    if (!state.current.busy && (!state.current.dirty || window.confirm('Discard unsaved changes?')))
      closeRef.current();
  };
  useEffect(() => {
    const el = ref.current;
    if (!open || !el) return;
    const previous = document.activeElement as HTMLElement | null;
    if (el.showModal) el.showModal();
    else el.setAttribute('open', '');
    const focus =
      el.querySelector<HTMLElement>('[aria-invalid="true"]') ??
      el.querySelector<HTMLElement>(
        'input:not(:disabled),select:not(:disabled),textarea:not(:disabled)',
      ) ??
      el.querySelector<HTMLElement>('[data-initial-focus],button:not(:disabled)');
    focus?.focus();
    return () => {
      lastError.current = '';
      if (el.close) el.close();
      else el.removeAttribute('open');
      if (previous?.isConnected) previous.focus();
    };
  }, [open]);
  useEffect(() => {
    const el = ref.current;
    const error = el?.querySelector<HTMLElement>('[role="alert"]');
    const text = error?.textContent ?? '';
    if (text && text !== lastError.current) {
      const invalid = el?.querySelector<HTMLElement>('[aria-invalid="true"]');
      if (invalid) invalid.focus();
      else {
        error?.setAttribute('tabindex', '-1');
        error?.focus();
      }
    }
    lastError.current = text;
  });
  return open ? (
    <dialog
      ref={ref}
      className={className}
      aria-labelledby={id}
      aria-describedby={description ? `${id}-description` : undefined}
      aria-busy={busy || undefined}
      onCancel={(e) => {
        e.preventDefault();
        close();
      }}
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          close();
        }
        if (e.key === 'Tab') {
          const els = Array.from(
            ref.current?.querySelectorAll<HTMLElement>(
              'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),a[href],[tabindex="0"]',
            ) ?? [],
          ).filter((el) => !el.hidden);
          const first = els[0],
            last = els.at(-1);
          if (e.shiftKey && document.activeElement === first) {
            e.preventDefault();
            last?.focus();
          } else if (!e.shiftKey && document.activeElement === last) {
            e.preventDefault();
            first?.focus();
          }
        }
      }}
    >
      <header className="dialog-header">
        <div>
          <h2 id={id}>{title}</h2>
          {description ? (
            <p className="muted" id={`${id}-description`}>
              {description}
            </p>
          ) : null}
        </div>
        <Button
          variant="quiet"
          size="icon"
          aria-label={`Close ${title}`}
          disabled={busy}
          onClick={close}
        >
          ×
        </Button>
      </header>
      <div className="dialog-body stack">{children}</div>
      <footer className="dialog-footer">
        <Button variant="secondary" disabled={busy} onClick={close}>
          {cancelLabel}
        </Button>
        {footer}
      </footer>
    </dialog>
  ) : null;
}
