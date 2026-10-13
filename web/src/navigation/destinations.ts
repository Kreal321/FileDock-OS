import type { NavigationDestination } from '../api/view-models.ts';
export type Route = { id: string; operationId?: string; unavailable?: boolean };
export const destinations: NavigationDestination[] = [
  {
    id: 'dashboard',
    hashRoute: '#/dashboard',
    label: 'Dashboard',
    icon: 'dashboard',
    group: 'main',
    availability: 'implemented',
    requiredView: 'DashboardPage',
  },
  {
    id: 'settings',
    hashRoute: '#/settings',
    label: 'Settings',
    icon: 'settings',
    group: 'main',
    availability: 'implemented',
    requiredView: 'SettingsPage',
  },
  ...[
    { id: 'administrators', label: 'Administrators', icon: 'admin', requiredView: 'AdminsPage' },
    { id: 'tokens', label: 'API tokens', icon: 'token', requiredView: 'TokensPage' },
    { id: 'network', label: 'Network settings', icon: 'network', requiredView: 'NetworkPage' },
    { id: 'operations', label: 'Operations', icon: 'operations', requiredView: 'OperationsPage' },
    { id: 'audit', label: 'Audit', icon: 'audit', requiredView: 'AuditPage' },
  ].map((d) => ({
    ...d,
    hashRoute: `#/settings/${d.id}`,
    group: 'settings' as const,
    availability: 'implemented' as const,
  })),
];
export function resolveRoute(hash: string): Route {
  if (!hash || hash === '#' || hash === '#/dashboard') return { id: 'dashboard' };
  const known = destinations.find((d) => d.hashRoute === hash);
  if (known) return { id: known.id };
  const detail = hash.match(/^#\/settings\/operations\/([^/]+)$/);
  if (detail) {
    try {
      return { id: 'operation-detail', operationId: decodeURIComponent(detail[1]!) };
    } catch {
      /* Invalid encoding is an unavailable page. */
    }
  }
  return { id: 'dashboard', unavailable: true };
}
