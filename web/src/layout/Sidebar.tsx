import { useEffect, useRef } from 'react';
import { destinations, type Route } from '../navigation/destinations.ts';
import { Icon } from '../components/Icon.tsx';
export function Sidebar({
  route,
  open,
  onClose,
}: {
  route: Route;
  open: boolean;
  onClose: () => void;
}) {
  const settings = useRef<HTMLDetailsElement>(null);
  const current = route.id === 'operation-detail' ? 'operations' : route.id;
  const isSettings =
    route.id === 'settings' || destinations.some((d) => d.group === 'settings' && d.id === current);
  useEffect(() => {
    if (isSettings && settings.current) settings.current.open = true;
  }, [isSettings, route.id]);
  const link = (id: string, label: string, href: string, icon?: string) => (
    <a
      key={id}
      className="nav-link"
      href={href}
      aria-current={current === id ? 'page' : undefined}
      onClick={onClose}
    >
      {icon ? <Icon name={icon} /> : null}
      {label}
    </a>
  );
  return (
    <aside className={`sidebar ${open ? 'mobile-open' : ''}`}>
      <a className="brand" href="#/dashboard">
        <Icon name="folder" />
        FileDock
      </a>
      <nav
        id="main-navigation"
        aria-label="Main navigation"
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            onClose();
          }
        }}
      >
        {['dashboard', 'folders', 'extensions'].map((id) => {
          const d = destinations.find((d) => d.id === id)!;
          return link(d.id, d.label, d.hashRoute, d.icon);
        })}
        <details className="nav-group" ref={settings}>
          <summary className="nav-link nav-group-toggle">
            <Icon name="settings" />
            Settings
            <span className="nav-chevron" aria-hidden="true">
              ⌄
            </span>
          </summary>
          <div className="nav-children">
            {link('settings', 'Overview', '#/settings')}
            {destinations
              .filter((d) => d.group === 'settings')
              .map((d) => link(d.id, d.label, d.hashRoute, d.icon))}
          </div>
        </details>
      </nav>
      <p className="sidebar-footer">
        FileDock OS
        <br />
        Appliance management
      </p>
    </aside>
  );
}
