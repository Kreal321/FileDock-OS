import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, expect, test, vi } from 'vitest';
import { api } from '../../api/client.ts';
import { PoolsPage } from './PoolsPage.tsx';
vi.mock('../../api/client.ts', () => ({ api: { get: vi.fn(), post: vi.fn(), put: vi.fn() } }));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
function setup(canMutate = true) {
  vi.mocked(api.get).mockImplementation(async (path) => {
    if (path === '/disks')
      return {
        items: [
          {
            serial: 'DATA-001',
            devicePath: '/dev/vdb',
            model: 'Test disk',
            sizeBytes: 20e9,
            isBlank: true,
            isSystemDisk: false,
            poolId: '',
            health: 'Unknown',
          },
        ],
      } as never;
    if (path === '/pools/system')
      return {
        generation: 1,
        available: false,
        unavailableReason: 'System disk too small',
        maxUsagePercent: 50,
        reservedFreeBytes: '100000000000',
        writable: 'WRITABLE_UNSPECIFIED',
      } as never;
    if (path === '/system/storage-settings')
      return { generation: 1, warningPercent: 90, criticalPercent: 95 } as never;
    return { items: [], nextPageToken: null } as never;
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <PoolsPage canMutate={canMutate} />
    </QueryClientProvider>,
  );
  return client;
}
test('disk erasure requires the exact displayed serial and preserves the form on refusal', async () => {
  setup();
  await screen.findByText('DATA-001');
  fireEvent.click(screen.getByRole('button', { name: 'Create pool' }));
  fireEvent.change(screen.getByLabelText('Pool name'), { target: { value: 'media' } });
  fireEvent.click(screen.getByRole('checkbox', { name: /DATA-001/ }));
  const submit = screen.getByRole('button', { name: 'Erase selected disks and continue' });
  expect(submit).toBeDisabled();
  fireEvent.change(screen.getByLabelText('Type ERASE DATA-001'), {
    target: { value: 'ERASE DATA-001' },
  });
  expect(submit).not.toBeDisabled();
  vi.mocked(api.post).mockRejectedValueOnce(new Error('disk.not_blank'));
  fireEvent.click(submit);
  await screen.findByRole('alert');
  expect(screen.getByLabelText('Pool name')).toHaveValue('media');
  expect(api.post).toHaveBeenCalledWith('/pools', {
    name: 'media',
    diskSerials: ['DATA-001'],
    typedConfirmation: 'ERASE DATA-001',
  });
});
test('read-only mode disables destructive actions and shows unavailable capacity honestly', async () => {
  setup(false);
  await screen.findByText('System disk too small');
  expect(screen.getByRole('button', { name: 'Create pool' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Adopt filesystem' })).toBeDisabled();
});
test('background pool refetch does not erase a typed draft', async () => {
  const client = setup();
  await screen.findByText('DATA-001');
  fireEvent.click(screen.getByRole('button', { name: 'Create pool' }));
  fireEvent.change(screen.getByLabelText('Pool name'), { target: { value: 'draft' } });
  await client.invalidateQueries({ queryKey: ['pools'] });
  await waitFor(() => expect(screen.getByLabelText('Pool name')).toHaveValue('draft'));
});
