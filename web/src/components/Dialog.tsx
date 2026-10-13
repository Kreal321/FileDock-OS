import { useEffect, useId, useRef, type ReactNode } from 'react';
import { Button } from './Button.tsx';
export function Dialog({
  open,
  title,
  onClose,
  children,
  busy = false,
}: {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  busy?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const id = useId();
  const closeRef = useRef(onClose);
  useEffect(() => {
    closeRef.current = onClose;
  }, [onClose]);
  useEffect(() => {
    const element = ref.current;
    if (!open || !element) return;
    const previous = document.activeElement as HTMLElement | null;
    if (element.showModal) element.showModal();
    else element.setAttribute('open', '');
    const focus = element.querySelector<HTMLElement>('button,input,select,textarea,[tabindex="0"]');
    focus?.focus();
    return () => {
      if (element.close) element.close();
      else element.removeAttribute('open');
      previous?.focus();
    };
  }, [open]);
  return open ? (
    <dialog
      ref={ref}
      aria-labelledby={id}
      aria-busy={busy || undefined}
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) closeRef.current();
      }}
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          if (!busy) closeRef.current();
        }
        if (e.key === 'Tab') {
          const els = Array.from(
            ref.current?.querySelectorAll<HTMLElement>(
              'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex="0"]',
            ) ?? [],
          );
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
      <div className="stack">
        <h2 id={id}>{title}</h2>
        {children}
        <Button variant="secondary" disabled={busy} onClick={onClose}>
          Cancel
        </Button>
      </div>
    </dialog>
  ) : null;
}
