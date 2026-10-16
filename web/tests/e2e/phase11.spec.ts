import { expect, test } from '@playwright/test';
import { status, health, identity } from '../fixtures/ui-states.ts';

for (const width of [1440, 360]) {
  test(`Phase 11 navigation, dialogs and mutation guards at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1000 });
    let socket: import('@playwright/test').WebSocketRoute | undefined;
    await page.route('**/api/v1/**', async (route) => {
      const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
      let data: unknown = { items: [], nextPageToken: null };
      if (path === '/system/status') data = status;
      else if (path === '/system/core-health') data = health;
      else if (path === '/auth/me') data = identity;
      else if (path === '/pools/system')
        data = {
          available: false,
          generation: 1,
          maxUsagePercent: 50,
          reservedFreeBytes: '100000000000',
          unavailableReason: 'The system disk cannot retain the configured reserve.',
        };
      else if (path === '/system/storage-settings')
        data = { generation: 1, warningPercent: 90, criticalPercent: 95 };
      else if (path === '/disks')
        data = {
          items: [
            {
              serial: 'DATA-001',
              devicePath: '/dev/vdb',
              model: 'Test disk',
              sizeBytes: 20000000000,
              isBlank: true,
              isSystemDisk: false,
              health: 'Healthy',
            },
          ],
        };
      else if (path === '/pools')
        data = {
          items: [
            {
              id: 'pool-fixture',
              generation: 1,
              spec: { name: 'media', kind: 'Lvm' },
              status: { state: 'Ready', capacityBytes: 20000000000, usedBytes: 1000000000 },
            },
          ],
          nextPageToken: null,
        };
      else if (path === '/connections')
        data = {
          items: [
            {
              id: 'connection-fixture',
              generation: 1,
              spec: { name: 'Archive', providerFamily: 's3-compatible' },
              status: { lastTest: { connectorId: 'fd-s3' } },
            },
          ],
          nextPageToken: null,
        };
      else if (path === '/share-users')
        data = {
          items: [
            {
              id: 'share-fixture',
              generation: 1,
              spec: { username: 'reader', enabled: true },
              status: { state: 'Ready' },
            },
          ],
          nextPageToken: null,
        };
      else if (path === '/apps')
        data = {
          items: [
            {
              id: 'app-fixture',
              generation: 1,
              spec: { name: 'media-reader', compose: 'services: {}' },
              status: { state: 'Running' },
            },
          ],
          nextPageToken: null,
        };
      else if (path === '/connections/options')
        data = {
          connectors: [
            { subsystemId: 'native', enabled: true, families: [{ family: 's3-compatible' }] },
          ],
        };
      await route.fulfill({ json: data });
    });
    await page.route('**/hubs/events/negotiate*', (route) =>
      route.fulfill({
        json: {
          negotiateVersion: 1,
          connectionId: 'fixture',
          connectionToken: 'fixture',
          availableTransports: [{ transport: 'WebSockets', transferFormats: ['Text'] }],
        },
      }),
    );
    await page.routeWebSocket('**/hubs/events*', (ws) => {
      socket = ws;
      ws.onMessage((message) => {
        if (String(message).includes('protocol')) {
          ws.send('{}\u001e');
          ws.send(
            JSON.stringify({ type: 1, target: 'core-health', arguments: [health] }) + '\u001e',
          );
        }
      });
    });
    await page.goto('/#/pools');
    await expect(page.getByRole('heading', { name: 'Pools & disks', exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Create pool', exact: true }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    await page.getByLabel('Pool name').fill('media');
    await dialog.getByRole('checkbox').check();
    const erase = dialog.getByRole('button', { name: 'Erase selected disks and continue' });
    await expect(erase).toBeDisabled();
    await page.getByLabel('Type ERASE DATA-001').fill('ERASE DATA-001');
    await expect(erase).toBeEnabled();
    socket?.send(
      JSON.stringify({
        type: 1,
        target: 'core-health',
        arguments: [{ state: 'Unreachable', canMutate: false, detail: 'Test outage' }],
      }) + '\u001e',
    );
    await expect(erase).toBeDisabled();
    await expect(page.getByLabel('Pool name')).toHaveValue('media');
    await page.screenshot({
      path: `test-results/phase11-create-pool-${width}.png`,
      fullPage: true,
    });
    // Reload to discard the synthetic dirty form and restore the connection.
    for (const [route, title, button, dialogTitle] of [
      ['connections', 'Cloud connections', 'Add connection', 'Add cloud connection'],
      ['shares', 'Shares', 'Add share user', 'Add share user'],
      ['apps', 'Docker apps', 'Add application', 'Add application'],
    ]) {
      await page.goto(`/?review=${route}#/${route}`);
      await expect(page.getByRole('heading', { name: title, exact: true })).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      );
      await page.screenshot({ path: `test-results/phase11-${route}-${width}.png`, fullPage: true });
      await page.getByRole('button', { name: button, exact: true }).click();
      await expect(page.getByRole('dialog', { name: dialogTitle, exact: true })).toBeVisible();
      await page.screenshot({
        path: `test-results/phase11-${route}-dialog-${width}.png`,
        fullPage: true,
      });
    }
  });
}
