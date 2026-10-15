import { useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Button } from './Button.tsx';
export type RowAction = {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
};
export function RowActions({ label, actions }: { label: string; actions: RowAction[] }) {
  const [position, setPosition] = useState<{ top: number; right: number } | null>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const id = useId();
  function close(restore = false) {
    setPosition(null);
    if (restore) trigger.current?.focus();
  }
  useEffect(() => {
    if (!position) return;
    const dismiss = (e: PointerEvent) => {
      if (
        !panel.current?.contains(e.target as Node) &&
        !trigger.current?.contains(e.target as Node)
      )
        setPosition(null);
    };
    const escape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setPosition(null);
        trigger.current?.focus();
      }
    };
    const moved = () => setPosition(null);
    const focus = (e: FocusEvent) => {
      if (
        !panel.current?.contains(e.target as Node) &&
        !trigger.current?.contains(e.target as Node)
      )
        setPosition(null);
    };
    document.addEventListener('pointerdown', dismiss);
    document.addEventListener('keydown', escape);
    document.addEventListener('focusin', focus);
    window.addEventListener('resize', moved);
    window.addEventListener('scroll', moved, true);
    panel.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
    return () => {
      document.removeEventListener('pointerdown', dismiss);
      document.removeEventListener('keydown', escape);
      document.removeEventListener('focusin', focus);
      window.removeEventListener('resize', moved);
      window.removeEventListener('scroll', moved, true);
    };
  }, [position]);
  return (
    <>
      <Button
        ref={trigger}
        variant="quiet"
        size="icon"
        aria-label={label}
        aria-expanded={!!position}
        aria-controls={position ? id : undefined}
        onClick={() => {
          if (position) {
            close();
            return;
          }
          const r = trigger.current!.getBoundingClientRect();
          setPosition({
            top: Math.max(8, Math.min(r.bottom + 4, window.innerHeight - actions.length * 44 - 24)),
            right: Math.max(8, window.innerWidth - r.right),
          });
        }}
      >
        ⋯
      </Button>
      {position
        ? createPortal(
            <div
              id={id}
              ref={panel}
              className="row-actions-popover"
              role="group"
              aria-label={label}
              style={{ top: position.top, right: position.right }}
            >
              {actions.map((a) => (
                <Button
                  key={a.label}
                  variant="quiet"
                  className={a.danger ? 'text-danger' : ''}
                  disabled={a.disabled}
                  onClick={() => {
                    close(true);
                    a.onClick();
                  }}
                >
                  {a.label}
                </Button>
              ))}
            </div>,
            document.body,
          )
        : null}
    </>
  );
}
