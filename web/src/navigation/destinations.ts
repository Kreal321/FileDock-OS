import type { NavigationDestination } from '../api/view-models.ts';
export type Route = { id: string; operationId?: string; unavailable?: boolean };
export const destinations: NavigationDestination[] = [
  ...[
    { id: 'pools', label: 'Pools & disks', icon: 'storage', requiredView: 'PoolsPage' },
    { id: 'connections', label: 'Cloud', icon: 'cloud', requiredView: 'ConnectionsPage' },
    { id: 'shares', label: 'Shares', icon: 'folder', requiredView: 'SharesPage' },
    { id: 'apps', label: 'Docker', icon: 'extension', requiredView: 'AppsPage' },
  ].map((d) => ({
    ...d,
    hashRoute: `#/${d.id}`,
    group: 'main' as const,
    availability: 'implemented' as const,
  })),
  {
    id: 'folders',
    hashRoute: '#/folders',
    label: 'Managed Folders',
    icon: 'folder',
    group: 'main',
    availability: 'implemented',
    requiredView: 'FoldersPage',
  },
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
    id: 'extensions',
    hashRoute: '#/extensions',
    label: 'Extensions',
    icon: 'extension',
    group: 'main',
    availability: 'implemented',
    requiredView: 'ExtensionsPage',
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
    {
      id: 'connectors',
      label: 'Connectors & routes',
      icon: 'cloud',
      requiredView: 'ConnectorsPage',
    },
    { id: 'network', label: 'Network settings', icon: 'network', requiredView: 'NetworkPage' },
    {
      id: 'notification-channels',
      label: 'Notifications',
      icon: 'network',
      requiredView: 'NotificationChannelsPage',
    },
    {
      id: 'state-protection',
      label: 'State protection',
      icon: 'settings',
      requiredView: 'StateProtectionPage',
    },
    { id: 'locations', label: 'Owned locations', icon: 'settings', requiredView: 'LocationsPage' },
    { id: 'operations', label: 'Operations', icon: 'operations', requiredView: 'OperationsPage' },
    { id: 'activity', label: 'Activity', icon: 'operations', requiredView: 'ActivityPage' },
    { id: 'alerts', label: 'Alerts', icon: 'warning', requiredView: 'AlertsPage' },
    { id: 'logs', label: 'Logs', icon: 'operations', requiredView: 'LogsPage' },
    { id: 'audit', label: 'Audit', icon: 'audit', requiredView: 'AuditPage' },
  ].map((d) => ({
    ...d,
    hashRoute: `#/settings/${d.id}`,
    group: 'settings' as const,
    parentId: 'settings',
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
