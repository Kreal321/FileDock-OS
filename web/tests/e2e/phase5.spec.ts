import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';

const host = process.env.FD_PHASE5_SSH_HOST;
const user = process.env.FD_PHASE5_SSH_USER;
const key = process.env.FD_PHASE5_SSH_KEY;
function root(command: string) {
  execFileSync(
    'ssh',
    [
      '-i',
      key!,
      '-o',
      'BatchMode=yes',
      '-o',
      'StrictHostKeyChecking=no',
      '-o',
      'UserKnownHostsFile=/dev/null',
      `${user}@${host}`,
      `sudo ${command}`,
    ],
    { timeout: 15_000 },
  );
}

test('live subsystem state and Core loss disable changes and recover without refreshing', async ({
  page,
}) => {
  test.skip(!host || !user || !key, 'Requires the disposable Phase 5 VM fixture and SSH key');
  test.setTimeout(90_000);
  await page.goto('/');
  await page
    .getByLabel('Username', { exact: true })
    .fill(process.env.FD_TEST_ADMIN ?? 'phase5-admin');
  await page
    .getByLabel('Password', { exact: true })
    .fill(process.env.FD_TEST_PASSWORD ?? 'test-only');
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  const sample = page.getByRole('article').filter({ hasText: 'Phase 5 test subsystem' });
  await expect(sample.getByText('Healthy', { exact: true })).toBeVisible();
  // Killing an independent unit must be visible promptly, before the delayed restart.
  root('systemctl kill -s KILL filedock-phase5-sample');
  await expect(sample.getByText('Degraded', { exact: true })).toBeVisible({ timeout: 2000 });
  await expect(sample.getByText('Healthy', { exact: true })).toBeVisible({ timeout: 15_000 });
  try {
    root('systemctl stop filedock-core.socket filedock-core.service');
    await expect(page.getByRole('alert').first()).toContainText(/Core|connection/i, {
      timeout: 10_000,
    });
    await page.getByRole('link', { name: 'Settings', exact: true }).click();
    await page.getByRole('link', { name: 'Network settings', exact: true }).first().click();
    await expect(page.getByRole('button', { name: 'Save network settings' })).toBeDisabled();
  } finally {
    root('systemctl start filedock-core.socket filedock-core.service');
  }
  await expect(page.getByRole('button', { name: 'Save network settings' })).toBeEnabled({
    timeout: 30_000,
  });
});
