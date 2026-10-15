import type { ReactNode } from 'react';
export function TableToolbar({
  title,
  children,
  action,
}: {
  title?: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="table-toolbar">
      {title ? <h2>{title}</h2> : null}
      <div className="toolbar-controls">{children}</div>
      {action}
    </div>
  );
}
