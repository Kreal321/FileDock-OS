import {
  readViewContext,
  validateDates,
  type ViewFilters,
  type RecordType,
} from './viewContext.ts';
import type { NavigationDestination } from '../api/view-models.ts';
export type Route = {
  id: string;
  owner?: string;
  groupId?: string;
  section?: string;
  recordType?: RecordType;
  operationId?: string;
  resourceId?: string;
  unavailable?: boolean;
  filters?: ViewFilters;
  filterError?: string;
  privateReset?: boolean;
};
export const entryOwners: Record<string, { owner: string; groupId?: string; section?: string }> = {
  connections: { owner: 'connections', groupId: 'cloud-group' },
  connectors: { owner: 'connectors', groupId: 'cloud-group' },
  locations: { owner: 'locations', groupId: 'cloud-group' },
  settings: { owner: 'settings', groupId: 'settings-group' },
  administrators: { owner: 'access', groupId: 'settings-group', section: 'administrators' },
  tokens: { owner: 'access', groupId: 'settings-group', section: 'tokens' },
  network: { owner: 'network', groupId: 'settings-group' },
  operations: { owner: 'operations', groupId: 'operations-group' },
  alerts: { owner: 'alerts-delivery', groupId: 'operations-group', section: 'alerts' },
  'notification-channels': {
    owner: 'alerts-delivery',
    groupId: 'operations-group',
    section: 'delivery',
  },
  activity: { owner: 'events', groupId: 'operations-group' },
  logs: { owner: 'events', groupId: 'operations-group' },
  audit: { owner: 'events', groupId: 'operations-group' },
  'state-protection': { owner: 'state-protection' },
};
export const navigationGroups = [
  {
    id: 'cloud-group',
    label: 'Cloud',
    icon: 'cloud',
    children: [
      { id: 'connections', label: 'Connections', hash: '#/connections' },
      { id: 'connectors', label: 'Connectors & routes', hash: '#/settings/connectors' },
      { id: 'locations', label: 'Owned locations', hash: '#/settings/locations' },
    ],
  },
  {
    id: 'operations-group',
    label: 'Operations',
    icon: 'operations',
    children: [
      { id: 'operations', label: 'Jobs', hash: '#/settings/operations' },
      { id: 'alerts-delivery', label: 'Alerts & delivery', hash: '#/settings/alerts' },
      { id: 'events', label: 'Events', hash: '#/settings/activity' },
    ],
  },
  {
    id: 'settings-group',
    label: 'Settings',
    icon: 'settings',
    children: [
      { id: 'settings', label: 'Overview', hash: '#/settings' },
      { id: 'access', label: 'Access & security', hash: '#/settings/administrators' },
      { id: 'network', label: 'Network', hash: '#/settings/network' },
    ],
  },
];
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
    {
      id: 'administrators',
      label: 'Administrators',
      icon: 'admin',
      requiredView: 'AccessSecurityPage',
    },
    { id: 'tokens', label: 'API tokens', icon: 'token', requiredView: 'AccessSecurityPage' },
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
      requiredView: 'AlertsDeliveryPage',
    },
    {
      id: 'state-protection',
      label: 'State protection',
      icon: 'settings',
      requiredView: 'StateProtectionPage',
    },
    { id: 'locations', label: 'Owned locations', icon: 'settings', requiredView: 'LocationsPage' },
    { id: 'operations', label: 'Operations', icon: 'operations', requiredView: 'OperationsPage' },
    { id: 'activity', label: 'Activity', icon: 'operations', requiredView: 'EventsPage' },
    { id: 'alerts', label: 'Alerts', icon: 'warning', requiredView: 'AlertsDeliveryPage' },
    { id: 'logs', label: 'Logs', icon: 'operations', requiredView: 'EventsPage' },
    { id: 'audit', label: 'Audit', icon: 'audit', requiredView: 'EventsPage' },
  ].map((d) => ({
    ...d,
    hashRoute: `#/settings/${d.id}`,
    group: 'settings' as const,
    parentId: 'settings',
    availability: 'implemented' as const,
  })),
];
export function resolveRoute(hash: string): Route {
  const [path, rawQuery = ''] = (hash || '#/dashboard').split('?');
  const entry = destinations.find(
    (d) => d.hashRoute === path || ((path === '#' || path === '') && d.id === 'dashboard'),
  );
  let route: Route;
  if (entry) route = { id: entry.id, ...(entryOwners[entry.id] ?? { owner: entry.id }) };
  else {
    const patterns = [
      { re: /^#\/extensions\/([^/]+)\/upgrade$/, id: 'extension-package', owner: 'extensions' },
      {
        re: /^#\/settings\/operations\/([^/]+)$/,
        id: 'operation-detail',
        owner: 'operations',
        groupId: 'operations-group',
      },
      { re: /^#\/folders\/([^/]+)\/edit$/, id: 'folder-edit', owner: 'folders' },
    ];
    const dynamic = patterns.find((p) => p.re.test(path!));
    if (dynamic) {
      try {
        const value = decodeURIComponent(path!.match(dynamic.re)![1]!);
        route = {
          id: dynamic.id,
          owner: dynamic.owner,
          groupId: dynamic.groupId,
          ...(dynamic.id === 'operation-detail' ? { operationId: value } : { resourceId: value }),
        };
      } catch {
        return { id: 'dashboard', unavailable: true };
      }
    } else if (path === '#/folders/new') route = { id: 'folder-new', owner: 'folders' };
    else if (path === '#/extensions/package')
      route = { id: 'extension-package', owner: 'extensions' };
    else if (path === '#/settings/state-protection/connection')
      route = { id: 'recovery-connection', owner: 'state-protection' };
    else return { id: 'dashboard', unavailable: true };
  }
  const params = new URLSearchParams(rawQuery);
  const type = ['activity', 'logs', 'audit'].includes(route.id)
    ? (route.id as RecordType)
    : undefined;
  if (type) route.recordType = type;
  const allowed = type
    ? [
        'pageSize',
        'pageToken',
        'context',
        ...(type === 'activity' ? [] : ['since', 'until']),
        ...(type === 'logs' ? ['minSeverity'] : []),
      ]
    : [];
  const filters: ViewFilters = {};
  for (const [key, value] of params) {
    if (!allowed.includes(key) || params.getAll(key).length > 1)
      route.filterError = 'Unsupported or repeated filter. Reset filters to continue.';
    else if (key !== 'context') filters[key] = value;
  }
  if (
    filters.pageSize &&
    (!/^\d+$/.test(filters.pageSize) ||
      Number(filters.pageSize) < 1 ||
      Number(filters.pageSize) > 500)
  )
    route.filterError = 'Page size must be between 1 and 500.';
  if (
    filters.minSeverity &&
    !['Debug', 'Info', 'Warning', 'Error', 'Critical'].includes(filters.minSeverity)
  )
    route.filterError = 'Unknown minimum severity.';
  route.filterError ??= validateDates(filters);
  const context = params.get('context');
  if (context && type) {
    const privateValues = readViewContext(context, type);
    if (privateValues) Object.assign(filters, privateValues);
    else {
      route.privateReset = true;
      delete filters.pageToken;
    }
  }
  if (type) route.filters = filters;
  return route;
}
