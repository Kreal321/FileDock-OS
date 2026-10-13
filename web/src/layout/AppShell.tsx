import { useQuery } from '@tanstack/react-query';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { api } from '../api/client.ts';
import { identityView } from '../api/view-models.ts';
import { useLiveCoreStatus } from '../api/useLiveCoreStatus.ts';
import { CoreStatusBanner } from '../components/CoreStatusBanner.tsx';
import { Notice } from '../components/Notice.tsx';
import type { Route } from '../navigation/destinations.ts';
import { destinations } from '../navigation/destinations.ts';
import { Sidebar } from './Sidebar.tsx';
import { TopBar } from './TopBar.tsx';
export function AppShell({
  route,
  onSignOut,
  children,
}: {
  route: Route;
  onSignOut: () => void;
  children:
    ReactNode | ((canMutate: boolean, connected: boolean, browserConnected: boolean) => ReactNode);
}) {
  const [open, setOpen] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null);
  const main = useRef<HTMLElement>(null);
  const health = useLiveCoreStatus();
  useEffect(() => {
    if (open)
      document.getElementById('main-navigation')?.querySelector<HTMLAnchorElement>('a')?.focus();
  }, [open]);
  const me = useQuery({
    queryKey: ['me'],
    queryFn: () => api.get<{ id: string; username: string }>('/auth/me'),
    retry: false,
  });
  const close = () => {
    setOpen(false);
    if (open) toggle.current?.focus();
  };
  return (
    <div
      className="appliance-shell"
      onKeyDown={(e) => {
        if (open && e.key === 'Escape') {
          e.preventDefault();
          close();
        }
      }}
    >
      <a
        className="skip-link"
        href="#main-content"
        onClick={(e) => {
          e.preventDefault();
          main.current?.focus();
        }}
      >
        Skip to content
      </a>
      <Sidebar route={route} open={open} onClose={close} />
      <div className="shell-body">
        <TopBar
          identity={me.data ? identityView(me.data) : undefined}
          onSignOut={onSignOut}
          open={open}
          onToggle={() => setOpen(!open)}
          toggleRef={toggle}
        />
        <main id="main-content" ref={main} tabIndex={-1} className="main-content stack">
          <CoreStatusBanner health={health} />
          {me.error ? (
            <Notice tone="danger" role="status">
              Account information unavailable. {me.error.message}
            </Notice>
          ) : null}
          {route.unavailable ? (
            <Notice tone="info">This page is unavailable. Showing Dashboard.</Notice>
          ) : null}
          {route.id !== 'dashboard' ? (
            <nav className="settings-nav" aria-label="Settings navigation">
              <a href="#/settings" aria-current={route.id === 'settings' ? 'page' : undefined}>
                Overview
              </a>
              {destinations
                .filter((d) => d.group === 'settings')
                .map((d) => (
                  <a
                    key={d.id}
                    href={d.hashRoute}
                    aria-current={
                      route.id === d.id ||
                      (d.id === 'operations' && route.id === 'operation-detail')
                        ? 'page'
                        : undefined
                    }
                  >
                    {d.label}
                  </a>
                ))}
            </nav>
          ) : null}
          {typeof children === 'function'
            ? children(
                health?.canMutate === true,
                health?.state === 'Healthy' && health?.browserConnected === true,
                health?.browserConnected === true,
              )
            : children}
        </main>
      </div>
    </div>
  );
}
