import { expect, test, type Page } from '@playwright/test';
import {
  status,
  health,
  identity,
  network,
  operation,
  activity,
  layoutStates,
} from '../fixtures/ui-states.ts';
async function appliance(page: Page) {
  await page.clock.setFixedTime(new Date('2026-10-15T12:00:00Z'));
  await page.route('**/api/v1/**', async (route) => {
    const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
    const method = route.request().method();
    let data: unknown = {};
    if (path === '/system/status') data = status;
    else if (path === '/system/core-health') data = health;
    else if (path === '/auth/me') data = identity;
    else if (path === '/admins')
      data =
        method === 'GET'
          ? [
              { ...identity, enabled: true, lockedUntil: null },
              { id: 'other', username: 'kyson', enabled: true, lockedUntil: null },
            ]
          : { id: 'new' };
    else if (path === '/tokens')
      data =
        method === 'POST'
          ? { token: 'SYNTHETIC-TOKEN-REDACTED' }
          : [
              {
                id: 'token',
                name: 'Backup automation',
                scope: 'ReadOnly',
                expiresAt: null,
                lastUsedAt: null,
                revokedAt: null,
              },
            ];
    else if (path === '/system/network')
      data = method === 'PUT' ? { listenAddresses: ['192.0.2.22'] } : network;
    else if (path === '/restore/report') {
      await route.fulfill({ status: 204 });
      return;
    } else if (path === '/state-protection') data = layoutStates.recovery;
    else if (path === '/alerts') data = { items: [] };
    else if (path === '/operations') data = { operations: [operation], nextPageToken: null };
    else if (path.startsWith('/operations/')) data = operation;
    else if (path === '/audit') data = { items: [activity], nextPageToken: null };
    else if (path === '/folders') data = { items: [] };
    else if (path === '/folders/options')
      data = { modes: [], pools: [], connections: [], access: [] };
    else if (path === '/subsystems') data = { subsystems: [], defaultProviders: {} };
    else if (path === '/extensions/folders') data = { folders: [] };
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
  await page.routeWebSocket('**/hubs/events*', (ws) =>
    ws.onMessage((message) => {
      if (String(message).includes('protocol')) {
        ws.send('{}\u001e');
        ws.send(JSON.stringify({ type: 1, target: 'core-health', arguments: [health] }) + '\u001e');
      }
    }),
  );
}
for (const width of [1440, 360])
  test(`first UI increment at ${width}px`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 1000 });
    await appliance(page);
    await page.goto('/#/settings/administrators');
    await expect(page.getByRole('heading', { name: 'Administrators', exact: true })).toBeVisible();
    await expect(page.getByText(/Live connection unavailable/)).toHaveCount(0);
    if (width === 360) await page.getByRole('button', { name: 'Open navigation' }).click();
    await expect(page.locator('[aria-current="page"]')).toHaveCount(1);
    await expect(page.locator('[aria-current="page"]')).toHaveText('Administrators');
    expect(await page.locator('.settings-nav').count()).toBe(0);
    if (width === 360)
      await page.getByRole('navigation', { name: 'Main navigation' }).press('Escape');
    await expect(page.getByLabel('Username', { exact: true })).toHaveCount(0);
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({
      path: testInfo.outputPath(`administrators-${width}.png`),
      fullPage: true,
    });
    const headers = page.getByRole('columnheader');
    const cells = page.getByRole('row').nth(1).getByRole('cell');
    for (let i = 0; i < 4; i++) {
      const h = await headers.nth(i).boundingBox(),
        c = await cells.nth(i).boundingBox();
      expect(h?.x).toBe(c?.x);
    }
    await page.getByRole('button', { name: 'Add administrator', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Add administrator', exact: true });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByLabel('Username', { exact: true })).toBeFocused();
    await expect(dialog.getByRole('button', { name: 'Cancel', exact: true })).toHaveCount(1);
    await page.screenshot({
      path: testInfo.outputPath(`add-administrator-${width}.png`),
      fullPage: true,
    });
    for (const button of await dialog.getByRole('button').all()) {
      const rect = await button.boundingBox();
      expect(rect!.height).toBeGreaterThanOrEqual(44);
      expect(rect!.width).toBeGreaterThanOrEqual(44);
    }
    await dialog.getByLabel('Username', { exact: true }).fill('temporary');
    page.once('dialog', (d) => d.dismiss());
    await dialog.press('Escape');
    await expect(dialog).toBeVisible();
    page.once('dialog', (d) => d.accept());
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(dialog).toHaveCount(0);
    await expect(
      page.getByRole('button', { name: 'Add administrator', exact: true }),
    ).toBeFocused();
    // Native back must ask before discarding a draft, and cancellation must retain the form.
    await page.evaluate(() => {
      location.hash = '#/settings/network';
    });
    await expect(page.getByRole('heading', { name: 'Network settings' })).toBeVisible();
    await page.getByRole('button', { name: 'Configure network', exact: true }).click();
    await page.getByLabel('Listen addresses (one per line)').fill('192.0.2.22');
    page.once('dialog', (d) => d.dismiss());
    await page.goBack();
    await expect(page.getByLabel('Listen addresses (one per line)')).toHaveValue('192.0.2.22');
    await expect(page).toHaveURL(/settings\/network/);
    page.once('dialog', (d) => d.accept());
    await page.getByRole('dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.evaluate(() => {
      location.hash = '#/settings/tokens';
    });
    await expect(page.getByRole('heading', { name: 'API tokens' })).toBeVisible();
    await expect(page.getByLabel('Token name')).toHaveCount(0);
    await page.getByRole('button', { name: 'Create token', exact: true }).click();
    await page.getByLabel('Token name').fill('Example');
    await page
      .getByRole('dialog')
      .getByRole('button', { name: 'Create token', exact: true })
      .click();
    await expect(page.getByRole('dialog', { name: 'Save your token' })).toBeVisible();
    await page.getByRole('button', { name: 'I saved the token' }).click();
    await expect(page.getByText('SYNTHETIC-TOKEN-REDACTED')).toHaveCount(0);
    await page.evaluate(() => {
      location.hash = '#/settings';
    });
    await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath(`settings-${width}.png`), fullPage: true });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  });
test('dirty forward and navigation links use the same guard', async ({ page }) => {
  await appliance(page);
  await page.goto('/#/settings/administrators');
  await expect(page.getByRole('heading', { name: 'Administrators', exact: true })).toBeVisible();
  await page.evaluate(() => {
    location.hash = '#/settings/network';
  });
  await expect(page.getByRole('heading', { name: 'Network settings' })).toBeVisible();
  await page.goBack();
  await expect(page.getByRole('heading', { name: 'Administrators', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Add administrator' }).click();
  await page.getByLabel('Username', { exact: true }).fill('retain');
  page.once('dialog', (d) => d.dismiss());
  await page.goForward();
  await expect(page.getByLabel('Username', { exact: true })).toHaveValue('retain');
  await expect(page).toHaveURL(/settings\/administrators/);
  page.once('dialog', (d) => d.accept());
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('link', { name: 'Overview', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeVisible();
});
for (const width of [1440, 360])
  test(`dashboard and public forms retain shared density at ${width}px`, async ({
    page,
  }, testInfo) => {
    await page.setViewportSize({ width, height: 1000 });
    await appliance(page);
    await page.goto('/#/dashboard');
    await expect(page.getByRole('heading', { name: 'Dashboard', exact: true })).toBeVisible();
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: testInfo.outputPath(`dashboard-${width}.png`), fullPage: true });
    for (const code of [401, 423]) {
      await page.route('**/api/v1/system/status', (route) =>
        route.fulfill({ status: code, json: { detail: 'Test gate' } }),
      );
      await page.reload();
      if (code === 401) await expect(page.getByLabel('Username', { exact: true })).toBeVisible();
      else await expect(page.getByLabel('Setup code')).toBeVisible();
      await page.screenshot({
        path: testInfo.outputPath(`public-${code}-${width}.png`),
        fullPage: true,
      });
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      );
    }
  });
