import { expect, test } from '@playwright/test';

test('fresh appliance setup, recovery key, sign out, sign in and healthy dashboard', async ({
  page,
}) => {
  test.setTimeout(180_000);
  const code = process.env.FD_SETUP_CODE;
  if (!code)
    throw new Error('FD_SETUP_CODE is required: run filedockctl setup-code --json on the fresh VM');
  const username = process.env.FD_TEST_ADMIN ?? 'phase3-admin';
  const password = process.env.FD_TEST_PASSWORD ?? 'Phase3-Acceptance-Password!';
  await page.goto('/');
  await page.getByLabel('Setup code').fill(code);
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  await page.getByLabel('Appliance name').fill('phase3-test');
  await page.getByLabel('Administrator username').fill(username);
  await page.getByLabel('Password', { exact: true }).fill(password);
  await page.getByLabel('Re-enter password').fill(password);
  await page.getByRole('button', { name: 'Create administrator' }).click();
  const key = page.getByTestId('recovery-key');
  await expect(key).toBeVisible();
  expect((await page.request.get('/api/v1/system/status')).status()).toBe(423);
  const downloadPromise = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download recovery key and continue' }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe('filedock-recovery-key.txt');
  await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();
  await page.getByRole('button', { name: 'Sign out' }).click();
  await page.getByLabel('Username', { exact: true }).fill(username);
  await page.getByLabel('Password', { exact: true }).fill(password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  const expected = ['core', 'management', 'docker', 'samba', 'nfs', 'kopia', 'fd-s3', 'rclone'];
  await expect
    .poll(
      async () => {
        const response = await page.request.get('/api/v1/system/status');
        if (!response.ok()) return false;
        const status = await response.json();
        return expected.every((id) =>
          status.components.some(
            (c: { id: string; state: string }) => c.id === id && c.state === 'Healthy',
          ),
        );
      },
      { timeout: 90_000, message: 'All eight built-in components must be healthy' },
    )
    .toBe(true);
  await page.reload();
  await expect(page.locator('article.card')).toHaveCount(expected.length);
  await expect(page.locator('article.card > p:not(.eyebrow)')).toHaveText(
    expected.map(() => 'Healthy'),
  );
});
