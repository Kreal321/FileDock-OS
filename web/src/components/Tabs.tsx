import { useId, useRef, type ReactNode } from 'react';
export function Tabs({
  tabs,
  selected,
  onSelect,
  label = 'Sections',
  controls,
}: {
  tabs: { id: string; label: string; content: ReactNode }[];
  selected: string;
  onSelect: (id: string) => void;
  label?: string;
  controls?: ReactNode;
}) {
  const id = useId();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const current = tabs.find((tab) => tab.id === selected);
  return (
    <>
      <div className="tabs-toolbar">
        <div role="tablist" aria-label={label} className="tabs">
          {tabs.map((tab, index) => (
            <button
              key={tab.id}
              ref={(el) => {
                refs.current[index] = el;
              }}
              type="button"
              role="tab"
              id={`${id}-${tab.id}`}
              aria-controls={`${id}-panel-${tab.id}`}
              aria-selected={selected === tab.id}
              tabIndex={selected === tab.id ? 0 : -1}
              onClick={() => onSelect(tab.id)}
              onKeyDown={(e) => {
                const next =
                  e.key === 'ArrowRight'
                    ? (index + 1) % tabs.length
                    : e.key === 'ArrowLeft'
                      ? (index - 1 + tabs.length) % tabs.length
                      : e.key === 'Home'
                        ? 0
                        : e.key === 'End'
                          ? tabs.length - 1
                          : undefined;
                if (next !== undefined) {
                  e.preventDefault();
                  refs.current[next]?.focus();
                }
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onSelect(tab.id);
                }
              }}
            >
              {tab.label}
            </button>
          ))}
        </div>
        {controls}
      </div>
      {current ? (
        <div
          role="tabpanel"
          id={`${id}-panel-${current.id}`}
          aria-labelledby={`${id}-${current.id}`}
          tabIndex={0}
        >
          {current.content}
        </div>
      ) : null}
    </>
  );
}
