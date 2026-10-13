import { expect, test } from '@playwright/test';
import {
  components,
  status,
  health,
  identity,
  operation,
  activity,
  network,
  states,
} from '../fixtures/ui-states.ts';

test.use({
  locale: 'en-US',
  timezoneId: 'UTC',
  reducedMotion: 'reduce',
  trace: 'off',
  screenshot: 'off',
});
async function appliance(
  page: import('@playwright/test').Page,
  state: unknown = status,
  gate = 200,
) {
  let socket: import('@playwright/test').WebSocketRoute | undefined;
  await page.clock.setFixedTime(new Date('2026-10-13T12:00:00Z'));
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/v1', '');
    let data: unknown = {};
    let code = 200;
    if (path === '/system/status') {
      data = state;
      code = gate;
    } else if (path === '/system/core-health') data = health;
    else if (path === '/auth/me') data = identity;
    else if (path === '/operations') data = { operations: [operation], nextPageToken: null };
    else if (path.startsWith('/operations/')) data = operation;
    else if (path === '/audit') data = { items: [activity], nextPageToken: null };
    else if (path === '/admins') data = [{ ...identity, enabled: true, lockedUntil: null }];
    else if (path === '/tokens') data = [];
    else if (path === '/system/network') data = network;
    else if (path === '/setup/complete')
      data = {
        username: 'root',
        applianceName: 'Test appliance',
        keyId: 'synthetic-key',
        recoveryKey: 'SYNTHETIC-RECOVERY-KEY',
      };
    await route.fulfill({ status: code, json: data });
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
      if (!String(message).includes('protocol')) return;
      ws.send('{}\u001e');
      ws.send(JSON.stringify({ type: 1, target: 'core-health', arguments: [health] }) + '\u001e');
    });
  });
  return {
    emit: (value: typeof health) =>
      socket?.send(
        JSON.stringify({ type: 1, target: 'core-health', arguments: [value] }) + '\u001e',
      ),
    close: () => socket?.close(),
  };
}
async function capture(page: import('@playwright/test').Page, name: string, liveExpected = true) {
  await page.evaluate(() => document.fonts.ready);
  if (liveExpected && !(await page.locator('.auth-layout').count()))
    await expect(page.getByText(/Live connection unavailable/)).toHaveCount(0);
  await expect(page).toHaveScreenshot(`${name}.png`, {
    fullPage: true,
    animations: 'disabled',
    mask: [page.getByTestId('recovery-key'), page.locator('[data-secret]')],
  });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
}
for (const width of [1440, 360]) {
  test(`current screens at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1000 });
    await appliance(page);
    await page.goto('/');
    await expect(page.getByRole('heading', { name: 'Dashboard', exact: true })).toBeVisible();
    await expect(page.getByText('root', { exact: true })).toBeVisible();
    for (const [route, title] of [
      ['dashboard', 'Dashboard'],
      ['settings', 'Settings'],
      ['settings/administrators', 'Administrators'],
      ['settings/tokens', 'API tokens'],
      ['settings/network', 'Network settings'],
      ['settings/operations', 'Operations'],
      ['settings/operations/op-one', 'Operation detail'],
      ['settings/audit', 'Audit'],
    ] as const) {
      await page.evaluate((hash) => {
        location.hash = hash;
      }, `#/${route}`);
      await expect(page.getByRole('heading', { name: title, exact: true })).toBeVisible();
      await capture(page, `${route.replaceAll('/', '-')}-${width}`);
    }
  });
  for (const gate of [401, 423, 503])
    test(`public gate ${gate} at ${width}px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 1000 });
      await appliance(page, status, gate);
      await page.goto('/#/settings/tokens');
      await expect(
        page.getByRole('heading', {
          name:
            gate === 401
              ? 'Sign in'
              : gate === 423
                ? 'Enter the one-time setup code'
                : 'Appliance unavailable',
        }),
      ).toBeVisible();
      await expect(page.getByRole('navigation')).toHaveCount(0);
      await capture(page, `gate-${gate}-${width}`);
      if (gate === 423) {
        await page.getByLabel('Setup code').fill('TEST');
        await page.getByRole('button', { name: 'Continue', exact: true }).click();
        await capture(page, `setup-admin-${width}`);
        await page.getByLabel('Password', { exact: true }).fill('synthetic');
        await page.getByLabel('Re-enter password').fill('different');
        await page.getByRole('button', { name: 'Create administrator' }).click();
        await expect(page.getByRole('alert')).toContainText('Passwords do not match');
        await capture(page, `setup-mismatch-${width}`);
        await page.getByLabel('Re-enter password').fill('synthetic');
        await page.getByRole('button', { name: 'Create administrator' }).click();
        await expect(page.getByTestId('recovery-key')).toBeVisible();
        await capture(page, `setup-recovery-${width}`);
      }
    });
}
for (const [name, state] of Object.entries(states))
  test(`truthful dashboard: ${name}`, async ({ page }) => {
    await appliance(page, state);
    await page.goto('/');
    await expect(page.getByRole('heading', { name: 'Dashboard', exact: true })).toBeVisible();
    await expect(page.getByText(/Protection information unavailable/)).toBeVisible();
    await capture(page, `dashboard-${name}`);
  });
test('local assets, supported navigation, history, and keyboard', async ({ page }) => {
  const external: string[] = [];
  await page.route('**/*', async (route) => {
    const origin = new URL(route.request().url()).origin;
    if (origin !== new URL(process.env.FD_BASE_URL ?? 'http://localhost:8021').origin) {
      external.push(route.request().url());
      await route.abort();
    } else await route.fallback();
  });
  await appliance(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Dashboard', exact: true })).toBeVisible();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: 'Skip to content' })).toBeFocused();
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeVisible();
  await page.goBack();
  await expect(page.getByRole('heading', { name: 'Dashboard', exact: true })).toBeVisible();
  await page.goto('/#/pools');
  await expect(page.getByText(/page is unavailable/)).toBeVisible();
  expect(external).toEqual([]);
  expect(await page.evaluate(() => document.fonts.check('16px Inter'))).toBe(true);
  await expect(page.getByRole('link', { name: 'Pools', exact: true })).toHaveCount(0);
  expect(components).toHaveLength(8);
});

for (const width of [1440, 360])
  test(`shared components and confirmation at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1000 });
    await appliance(page, status, 503);
    await page.goto('/');
    await expect(page.getByRole('heading', { name: 'Appliance unavailable' })).toBeVisible();
    await page.evaluate(async () => {
      // Vite serves this test-only harness; it is absent from the production module graph.
      // @ts-expect-error Test harness path is resolved by Vite in the browser.
      const module = await import(/* @vite-ignore */ '/tests/fixtures/component-gallery.tsx');
      module.mount();
    });
    await expect(page.getByRole('heading', { name: 'Component review' })).toBeVisible();
    await capture(page, `components-${width}`);
    await page.getByRole('button', { name: 'Open confirmation' }).click();
    await expect(page.getByRole('dialog')).toBeVisible();
    await capture(page, `confirmation-${width}`);
    await page.keyboard.press('Escape');
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Open confirmation' })).toBeFocused();
  });

test('live Core outage preserves dirty values, filters, inspection and sign-out', async ({
  page,
}) => {
  const live = await appliance(page);
  await page.goto('/#/settings/network');
  const save = page.getByRole('button', { name: 'Save network settings' });
  await expect(save).toBeEnabled();
  const input = page.getByLabel('Listen addresses (one per line)');
  await input.fill('192.0.2.22');
  live.emit({ state: 'Unreachable', detail: 'Core connection lost.', canMutate: false });
  await expect(save).toBeDisabled();
  await expect(input).toHaveValue('192.0.2.22');
  await expect(input).toBeFocused();
  await expect(page.getByRole('button', { name: 'Sign out', exact: true })).toBeEnabled();
  await page.getByRole('link', { name: 'Audit', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Apply filters' })).toBeEnabled();
  await page.getByRole('link', { name: 'Network settings', exact: true }).click();
  live.emit(health);
  await expect(save).toBeEnabled();
});

test('browser transport loss marks cached data and retains read-only controls', async ({
  page,
}) => {
  const live = await appliance(page);
  await page.goto('/#/settings/network');
  await expect(page.getByRole('button', { name: 'Save network settings' })).toBeEnabled();
  live.close();
  await expect(page.getByRole('button', { name: 'Save network settings' })).toBeDisabled();
  await expect(page.getByText(/Showing cached data/)).toBeVisible();
  await page.getByRole('link', { name: 'Audit', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Apply filters' })).toBeEnabled();
  await capture(page, 'browser-outage', false);
});

test('unexpected protected 401 ends the authenticated shell', async ({ page }) => {
  await appliance(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Dashboard', exact: true })).toBeVisible();
  await page.route('**/api/v1/tokens', (route) =>
    route.fulfill({ status: 401, json: { title: 'Session expired' } }),
  );
  await page.goto('/#/settings/tokens');
  await expect(page.getByRole('heading', { name: 'Sign in', exact: true })).toBeVisible();
  await expect(page.getByRole('navigation')).toHaveCount(0);
  expect(new URL(page.url()).hash).toBe('#/dashboard');
});

for (const mode of ['empty', 'error'])
  test(`read-only list states: ${mode}`, async ({ page }) => {
    await appliance(page);
    await page.route('**/api/v1/operations?*', (route) =>
      route.fulfill({
        status: mode === 'error' ? 503 : 200,
        json:
          mode === 'error'
            ? { title: 'Core unavailable' }
            : { operations: [], nextPageToken: null },
      }),
    );
    await page.route('**/api/v1/audit?*', (route) =>
      route.fulfill({
        status: mode === 'error' ? 503 : 200,
        json:
          mode === 'error' ? { title: 'Audit unavailable' } : { items: [], nextPageToken: null },
      }),
    );
    await page.goto('/#/settings/operations');
    await expect(page.getByRole('heading', { name: 'Operations', exact: true })).toBeVisible();
    await expect(
      page.getByText(mode === 'error' ? /Data unavailable/ : /No operations match/),
    ).toBeVisible();
    await capture(page, `operations-${mode}`);
    await page.getByRole('link', { name: 'Audit', exact: true }).click();
    await expect(
      page.getByText(mode === 'error' ? /Data unavailable/ : /No administrative activity matches/),
    ).toBeVisible();
    await capture(page, `audit-${mode}`);
  });
