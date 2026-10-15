import { expect, test } from '@playwright/test';
import { status, health, identity } from '../fixtures/ui-states.ts';
for (const width of [1440, 360])
  test(`log preview and complete details at ${width}px`, async ({ page }, info) => {
    const message =
      'fail: Microsoft.AspNetCore.Diagnostics.ExceptionHandlerMiddleware[1] restore.target_not_fresh\n' +
      'at FileDock.Management.Api.AdminSocket.RecoveryConsoleEndpoints in RecoveryConsoleEndpoints.cs:line 17\n'.repeat(
        80,
      );
    await page.setViewportSize({ width, height: 1000 });
    await page.clock.setFixedTime(new Date('2026-10-15T12:00:00Z'));
    await page.route('**/api/v1/**', (r) => {
      const path = new URL(r.request().url()).pathname.replace('/api/v1', '');
      return r.fulfill({
        json:
          path === '/system/status'
            ? status
            : path === '/system/core-health'
              ? health
              : path === '/auth/me'
                ? identity
                : path === '/logs'
                  ? {
                      items: [
                        {
                          at: '2026-10-15T12:00:00Z',
                          source: 'filedock-management.service',
                          severity: 'Error',
                          message,
                        },
                      ],
                    }
                  : path === '/activity'
                    ? {
                        items: [
                          {
                            seq: 3,
                            at: '2026-10-15T12:00:04Z',
                            type: 'state.protection.changed',
                            payload: { warning: true },
                          },
                          {
                            seq: 2,
                            at: '2026-10-15T12:00:02Z',
                            type: 'state.protection.changed',
                            payload: { warning: true },
                          },
                          {
                            seq: 1,
                            at: '2026-10-15T12:00:00Z',
                            type: 'state.protection.changed',
                            payload: { warning: false },
                          },
                        ],
                      }
                    : { items: [] },
      });
    });
    await page.route('**/hubs/events/negotiate*', (r) =>
      r.fulfill({
        json: {
          negotiateVersion: 1,
          connectionId: 'test',
          connectionToken: 'test',
          availableTransports: [{ transport: 'WebSockets', transferFormats: ['Text'] }],
        },
      }),
    );
    await page.routeWebSocket('**/hubs/events*', (ws) =>
      ws.onMessage((m) => {
        if (String(m).includes('protocol')) {
          ws.send('{}\u001e');
          ws.send(
            JSON.stringify({ type: 1, target: 'core-health', arguments: [health] }) + '\u001e',
          );
        }
      }),
    );
    await page.goto('/#/settings/logs');
    await expect(page.getByRole('heading', { name: 'Logs', exact: true })).toBeVisible();
    const trigger = page.getByRole('button', { name: /View log details from/ });
    await expect(trigger).toBeVisible();
    await page.evaluate(() => document.fonts.ready);
    expect(
      await page.locator('.log-message-preview').evaluate((el) => el.scrollWidth > el.clientWidth),
    ).toBe(true);
    expect((await trigger.boundingBox())!.height).toBe(44);
    expect(
      (await page.getByRole('columnheader', { name: 'Time', exact: true }).boundingBox())!.height,
    ).toBeLessThan(60);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
    await page.screenshot({ path: info.outputPath(`logs-${width}.png`), fullPage: true });
    await trigger.click();
    const dialog = page.getByRole('dialog', { name: 'Log details' });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByLabel('Full log message')).toHaveText(message);
    expect((await dialog.boundingBox())!.width).toBeLessThanOrEqual(width - 32);
    await page.screenshot({ path: info.outputPath(`log-details-${width}.png`), fullPage: true });
    await dialog.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(trigger).toBeFocused();
    await page.evaluate(() => {
      location.hash = '#/settings/activity';
    });
    await expect(page.getByText('2 identical updates', { exact: false })).toBeVisible();
    await expect(page.getByText('Recovery protection updated', { exact: true })).toHaveCount(2);
  });
