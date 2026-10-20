import type { Page } from '@playwright/test';
import {
  addonStates,
  status,
  identity,
  health,
  network,
  operation,
  layoutStates,
  folderOptionsFixture,
} from './ui-states.ts';
export async function addonAppliance(page: Page) {
  const requests: string[] = [];
  await page.clock.setFixedTime(new Date('2026-10-19T12:00:00Z'));
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url()),
      path = url.pathname.replace('/api/v1', '');
    requests.push(path);
    const responses: Record<string, unknown> = {
      '/system/status': status,
      '/system/core-health': health,
      '/auth/me': identity,
      '/admins': addonStates.administrators,
      '/tokens': addonStates.tokens,
      '/activity': addonStates.activity,
      '/logs': addonStates.logs,
      '/audit': addonStates.audit,
      '/notification-channels': layoutStates.channels,
      '/state-protection': layoutStates.recovery,
      '/locations': { locations: [] },
      '/system/network': network,
      '/operations': { operations: [operation] },
      '/subsystems': { subsystems: layoutStates.subsystems, defaultProviders: {} },
      '/extensions/folders': { folders: [] },
      '/folders/options': folderOptionsFixture,
      '/folders': { items: layoutStates.folders },
      '/connections/options': { connectors: [] },
      '/connectors': { connectors: [] },
      '/connector-routes': { generation: 1, defaults: [], overrides: [] },
      '/pools/system': { available: false, unavailableReason: 'Synthetic small disk' },
      '/system/storage-settings': { generation: 1, warningPercent: 90, criticalPercent: 95 },
    };
    if (path === '/restore/report') {
      await route.fulfill({ status: 204 });
      return;
    }
    if (path.startsWith('/operations/')) {
      await route.fulfill({ json: operation });
      return;
    }
    if (path.startsWith('/folders/') && !path.endsWith('/options')) {
      await route.fulfill({ json: layoutStates.folders[0] });
      return;
    }
    await route.fulfill({ json: responses[path] ?? { items: [], nextPageToken: null } });
  });
  await page.route('**/hubs/events/negotiate*', (r) =>
    r.fulfill({
      json: {
        negotiateVersion: 1,
        connectionId: 'fixture',
        connectionToken: 'fixture',
        availableTransports: [{ transport: 'WebSockets', transferFormats: ['Text'] }],
      },
    }),
  );
  await page.routeWebSocket('**/hubs/events*', (ws) =>
    ws.onMessage((message) => {
      if (String(message).includes('protocol')) {
        ws.send('{}\u001e');
        ws.send(JSON.stringify({ type: 1, target: 'core-health', arguments: [health] }) + '\u001e');
      }
    }),
  );
  return requests;
}
