import type { ReactNode } from 'react';
export function SettingsRow({
  title,
  description,
  action,
  children,
}: {
  title: string;
  description?: ReactNode;
  action?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-content">
        <h2>{title}</h2>
        {description ? <p className="muted">{description}</p> : null}
        {children}
      </div>
      {action ? <div className="settings-row-action">{action}</div> : null}
    </div>
  );
}
