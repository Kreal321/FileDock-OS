import type { ReactNode } from 'react';
export function SectionHeader({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <header className="page-header section-header">
      <div>
        <h2 tabIndex={-1}>{title}</h2>
        {description ? <p className="muted">{description}</p> : null}
      </div>
      {action}
    </header>
  );
}
