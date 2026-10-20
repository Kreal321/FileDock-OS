import { useEffect, useRef } from 'react';
import {
  destinations,
  entryOwners,
  navigationGroups,
  type Route,
} from '../navigation/destinations.ts';
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
  const nav = useRef<HTMLElement>(null);
  const mapping = entryOwners[route.id];
  const current =
    route.owner ?? mapping?.owner ?? (route.id === 'operation-detail' ? 'operations' : route.id);
  const groupId =
    route.groupId ??
    mapping?.groupId ??
    (route.id === 'operation-detail' ? 'operations-group' : undefined);
  useEffect(() => {
    if (groupId) {
      const group = nav.current?.querySelector<HTMLDetailsElement>(`[data-group="${groupId}"]`);
      if (group) group.open = true;
    }
  }, [groupId, route.id]);
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
  const group = (id: string) => {
    const g = navigationGroups.find((item) => item.id === id)!;
    return (
      <details className="nav-group" data-group={g.id} key={id}>
        <summary className="nav-link nav-group-toggle">
          <Icon name={g.icon} />
          {g.label}
          <span className="nav-chevron" aria-hidden="true">
            ⌄
          </span>
        </summary>
        <div className="nav-children">
          {g.children.map((child) => link(child.id, child.label, child.hash))}
        </div>
      </details>
    );
  };
  return (
    <aside className={`sidebar ${open ? 'mobile-open' : ''}`}>
      <a className="brand" href="#/dashboard">
        <Icon name="folder" />
        FileDock
      </a>
      <nav
        ref={nav}
        id="main-navigation"
        aria-label="Main navigation"
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            onClose();
          }
        }}
      >
        {[
          'dashboard',
          'folders',
          'pools',
          'cloud-group',
          'shares',
          'apps',
          'extensions',
          'state-protection',
          'operations-group',
          'settings-group',
        ].map((id) => {
          if (id.endsWith('-group')) return group(id);
          const d = destinations.find((item) => item.id === id)!;
          return link(id, id === 'state-protection' ? 'Recovery' : d.label, d.hashRoute, d.icon);
        })}
      </nav>
      <p className="sidebar-footer">
        FileDock OS
        <br />
        Appliance management
      </p>
    </aside>
  );
}
