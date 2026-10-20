import { expect, test, type Page, type TestInfo } from '@playwright/test';
import { addonAppliance } from '../fixtures/addon-appliance.ts';
import { destinations, resolveRoute } from '../../src/navigation/destinations.ts';
async function captureAddon(page: Page, info: TestInfo, name: string) {
  await page.waitForLoadState('networkidle');
  await page.evaluate(() => document.fonts.ready);
  if (process.env.FD_UI_CAPTURE_ONLY === '1')
    await page.screenshot({
      path: info.outputPath(name),
      fullPage: true,
      mask: [page.locator('[data-secret]')],
    });
  else
    await expect(page).toHaveScreenshot(name, {
      fullPage: true,
      animations: 'disabled',
      mask: [page.locator('[data-secret]')],
    });
}
for (const entry of destinations)
  test(`direct entry ${entry.hashRoute}`, async ({ page }) => {
    await addonAppliance(page);
    const errors: string[] = [];
    page.on('pageerror', (e) => errors.push(e.message));
    await page.goto('/' + entry.hashRoute);
    await expect(page.locator('main h1')).toHaveCount(1);
    await expect(page.locator('[aria-current=page]')).toHaveCount(1);
    const owner = resolveRoute(entry.hashRoute).owner;
    const ownerLabels: Record<string, string> = {
      access: 'Access & security',
      events: 'Events',
      'alerts-delivery': 'Alerts & delivery',
      operations: 'Jobs',
      'state-protection': 'Recovery',
      settings: 'Overview',
      network: 'Network',
    };
    await expect(page.locator('[aria-current=page]')).toHaveText(
      ownerLabels[owner!] ?? (entry.id === 'connections' ? 'Connections' : entry.label),
    );
    await expect(page.locator('[data-group=settings-group] a')).toHaveCount(3);
    await page.reload();
    await expect(page.locator('[aria-current=page]')).toHaveCount(1);
    expect(errors).toEqual([]);
  });
for (const width of [1440, 360])
  test(`Events toolbar and full-viewport dialog geometry ${width}`, async ({ page }, info) => {
    const requests = await addonAppliance(page);
    await page.setViewportSize({ width, height: 1000 });
    await page.goto('/#/settings/logs');
    await expect(page.getByRole('tab', { name: 'System logs' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    const trigger = page.getByRole('button', { name: /View log details from/ });
    await expect(trigger).toBeVisible();
    expect(requests).not.toContain('/activity');
    expect(requests).not.toContain('/audit');
    if (width === 1440) {
      const controls = [
        page.getByRole('tab', { name: 'Activity' }),
        page.getByLabel('Source', { exact: true }),
        page.getByLabel('Minimum severity'),
        page.getByRole('button', { name: 'Filters', exact: true }),
        page.getByRole('button', { name: 'Refresh', exact: true }),
      ];
      const boxes = await Promise.all(controls.map((c) => c.boundingBox()));
      const bottoms = boxes.map((b) => b!.y + b!.height);
      expect(Math.max(...bottoms) - Math.min(...bottoms)).toBeLessThanOrEqual(4);
    }
    await page.evaluate(() => document.fonts.ready);
    await captureAddon(page, info, `events-${width}.png`);
    await trigger.click();
    const dialog = page.getByRole('dialog', { name: 'Log details' });
    await expect(dialog).toBeVisible();
    const box = (await dialog.boundingBox())!;
    expect(Math.abs(box.x + box.width / 2 - width / 2)).toBeLessThanOrEqual(1);
    expect(Math.abs(box.y + box.height / 2 - 500)).toBeLessThanOrEqual(1);
    expect(box.x).toBeGreaterThanOrEqual(16);
    expect(box.height).toBeLessThanOrEqual(968);
    await captureAddon(page, info, `dialog-${width}.png`);
    await dialog.press('Escape');
    await expect(trigger).toBeFocused();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  });
test('private filters are memory-only, history restores and reload announces clearing', async ({
  page,
}) => {
  await addonAppliance(page);
  await page.goto('/#/settings/logs');
  await page.getByLabel('Source', { exact: true }).fill('synthetic-private-source');
  await page.getByLabel('Source', { exact: true }).press('Enter');
  await expect(page).toHaveURL(/context=/);
  expect(
    await page.evaluate(() =>
      JSON.stringify([location.hash, history.state, { ...localStorage }, { ...sessionStorage }]),
    ),
  ).not.toContain('synthetic-private-source');
  await page.getByRole('tab', { name: 'Audit', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Audit', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await page.goBack();
  await expect(page.getByLabel('Source', { exact: true })).toHaveValue('synthetic-private-source');
  await page.reload();
  await expect(
    page.getByText(
      'Private filters were cleared after reload. Re-enter them to refine these records.',
    ),
  ).toBeVisible();
  await expect(page.getByLabel('Source', { exact: true })).toHaveValue('');
  await expect(page).not.toHaveURL(/context=/);
});
test('six folder sections and long package review are full pages', async ({ page }, info) => {
  await addonAppliance(page);
  await page.goto('/#/folders/new');
  await expect(page.getByRole('heading', { name: '6. Advanced', exact: true })).toBeVisible();
  await expect(page.locator('form details')).toHaveCount(0);
  await page.getByLabel('Name', { exact: true }).fill('Media archive');
  await expect(page.getByLabel('FileDock Path')).toHaveValue('/storage/media-archive');
  await captureAddon(page, info, 'folder-editor.png');
  page.once('dialog', (d) => d.accept());
  await page.evaluate(() => {
    location.hash = '#/extensions/package';
  });
  await expect(page.getByLabel('Extension package (.fdext)')).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
});
test('manual record activation and query-scoped detail reset', async ({ page }) => {
  const requests = await addonAppliance(page);
  await page.goto('/#/settings/logs');
  const logs = page.getByRole('tab', { name: 'System logs' });
  await logs.focus();
  await logs.press('ArrowRight');
  await expect(page.getByRole('tab', { name: 'Audit', exact: true })).toBeFocused();
  await expect(logs).toHaveAttribute('aria-selected', 'true');
  expect(requests).not.toContain('/audit');
  await page.getByRole('tab', { name: 'Audit', exact: true }).press('Enter');
  await expect(page.getByRole('table', { name: 'Administrative activity' })).toBeVisible();
  await page.getByRole('tab', { name: 'System logs' }).click();
  await page.getByRole('button', { name: /View log details from/ }).click();
  await expect(page.getByRole('dialog', { name: 'Log details' })).toBeVisible();
  await page.evaluate(() => {
    location.hash = '#/settings/logs?minSeverity=Warning';
  });
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByLabel('Minimum severity')).toHaveValue('Warning');
});
test('extra filters apply as one validated snapshot and preserve applied values on Cancel', async ({
  page,
}) => {
  await addonAppliance(page);
  await page.goto('/#/settings/logs');
  await page.getByRole('button', { name: 'Filters', exact: true }).click();
  await page.getByLabel('Since (ISO timestamp)').fill('2026-10-18T00:00:00Z');
  await page.getByLabel('Containing text').fill('synthetic-private-text');
  await page.getByRole('button', { name: 'Apply filters' }).click();
  await expect(page.getByRole('button', { name: 'Filters (2)', exact: true })).toBeVisible();
  expect(page.url()).not.toContain('synthetic-private-text');
  await page.getByRole('button', { name: 'Filters (2)', exact: true }).click();
  await page.getByLabel('Containing text').fill('unapplied');
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('button', { name: 'Filters (2)', exact: true }).click();
  await expect(page.getByLabel('Containing text')).toHaveValue('synthetic-private-text');
});
for (const width of [1440, 360])
  test(`short action and constrained dialog ${width}`, async ({ page }) => {
    await addonAppliance(page);
    await page.setViewportSize({ width, height: 1000 });
    await page.goto('/#/settings/administrators');
    await page.getByRole('button', { name: 'Add administrator', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Add administrator', exact: true });
    const box = (await dialog.boundingBox())!;
    expect(Math.abs(box.x + box.width / 2 - width / 2)).toBeLessThanOrEqual(1);
    expect(Math.abs(box.y + box.height / 2 - 500)).toBeLessThanOrEqual(1);
    await page.setViewportSize({ width, height: 320 });
    const short = (await dialog.boundingBox())!;
    expect(short.y).toBeGreaterThanOrEqual(16);
    expect(short.height).toBeLessThanOrEqual(288);
    await expect(dialog.getByRole('button', { name: 'Cancel', exact: true })).toBeInViewport();
    await dialog.press('Escape');
    await expect(
      page.getByRole('button', { name: 'Add administrator', exact: true }),
    ).toBeFocused();
  });
test('a delayed one-time token cannot reappear after session expiry', async ({ page }) => {
  await addonAppliance(page);
  let finish: (() => void) | undefined;
  await page.route('**/api/v1/tokens', async (route) => {
    if (route.request().method() !== 'POST') {
      await route.fallback();
      return;
    }
    await new Promise<void>((resolve) => {
      finish = resolve;
    });
    await route.fulfill({ json: { token: 'SYNTHETIC-LATE-TOKEN' } }).catch(() => {});
  });
  await page.goto('/#/settings/tokens');
  await page.getByRole('button', { name: 'Create token', exact: true }).click();
  await page.getByLabel('Token name').fill('Synthetic delayed token');
  await page.getByRole('dialog').getByRole('button', { name: 'Create token', exact: true }).click();
  await expect.poll(() => !!finish).toBe(true);
  await page.evaluate(() => window.dispatchEvent(new Event('filedock:session-expired')));
  await expect(page.getByRole('heading', { name: 'Sign in', exact: true })).toBeVisible();
  finish!();
  await expect(page.getByText('SYNTHETIC-LATE-TOKEN')).toHaveCount(0);
  await expect(page.getByRole('dialog')).toHaveCount(0);
});
test('mobile tables keep readable headers and status labels inside a scrolling region', async ({
  page,
}) => {
  await addonAppliance(page);
  await page.setViewportSize({ width: 360, height: 1000 });
  for (const entry of ['administrators', 'audit', 'operations']) {
    await page.goto(`/#/settings/${entry}`);
    const table = page.locator('main table').first();
    await expect(table).toBeVisible();
    const metrics = await table.evaluate((el) => ({
      table: el.getBoundingClientRect().width,
      region: el.parentElement!.clientWidth,
      headerHeight: Math.max(
        ...Array.from(el.querySelectorAll('th')).map((th) => th.getBoundingClientRect().height),
      ),
      badges: Array.from(el.querySelectorAll('.badge')).map(
        (b) => b.getBoundingClientRect().height,
      ),
      page: document.documentElement.scrollWidth,
      viewport: innerWidth,
    }));
    expect(metrics.table).toBeGreaterThan(metrics.region);
    expect(metrics.headerHeight).toBeLessThanOrEqual(60);
    expect(metrics.badges.every((height) => height <= 44)).toBe(true);
    expect(metrics.page).toBeLessThanOrEqual(metrics.viewport);
  }
});
test('large event tables scroll within the record region with paging still reachable', async ({
  page,
}) => {
  await addonAppliance(page);
  await page.route('**/api/v1/logs?*', (route) =>
    route.fulfill({
      json: {
        items: Array.from({ length: 100 }, (_, i) => ({
          at: '2026-10-20T12:00:00Z',
          source: 'filedock-core.service',
          severity: 'Info',
          message: `Synthetic job ${i} finished`,
        })),
        nextPageToken: 'next',
      },
    }),
  );
  for (const width of [1440, 360]) {
    await page.setViewportSize({ width, height: 1000 });
    await page.goto('/#/settings/logs');
    const table = page.getByRole('region', { name: 'Journal entries' });
    await expect(table).toBeVisible();
    expect(await table.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
    await expect(page.getByRole('button', { name: 'Next', exact: true })).toBeInViewport();
    await table.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    await expect(page.getByRole('columnheader', { name: 'Time', exact: true })).toBeInViewport();
  }
});
test('cursor Previous uses matching query history and is disabled for a copied page', async ({
  page,
}) => {
  await addonAppliance(page);
  await page.goto('/#/settings/logs');
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(page).toHaveURL(/pageToken=/);
  await expect(page.getByRole('button', { name: 'Previous', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Previous', exact: true }).click();
  await expect(page).not.toHaveURL(/pageToken=/);
  await page.goto('/#/settings/logs?pageToken=copied');
  await expect(page.getByRole('button', { name: 'Previous', exact: true })).toBeDisabled();
});
