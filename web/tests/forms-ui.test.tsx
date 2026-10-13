import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { SetupWizard } from '../src/features/setup/SetupWizard.tsx';
import { NetworkPage } from '../src/features/settings/NetworkPage.tsx';
import { TokensPage } from '../src/features/tokens/TokensPage.tsx';
import { network } from './fixtures/ui-states.ts';
test('setup keeps editable root and password matching with advisory length', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}')));
  render(<SetupWizard onDone={() => {}} />);
  fireEvent.change(screen.getByLabelText('Setup code'), { target: { value: 'TEST' } });
  fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
  expect(await screen.findByLabelText('Administrator username')).toHaveValue('root');
  expect(screen.getByLabelText('Password')).not.toHaveAttribute('minlength');
  fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'a' } });
  fireEvent.change(screen.getByLabelText('Re-enter password'), { target: { value: 'b' } });
  fireEvent.click(screen.getByRole('button', { name: 'Create administrator' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Passwords do not match');
});
test('network refetch keeps dirty input and outage disables only mutations', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify(network))));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const { rerender } = render(
    <QueryClientProvider client={client}>
      <NetworkPage canMutate />
    </QueryClientProvider>,
  );
  const input = await screen.findByLabelText('Listen addresses (one per line)');
  fireEvent.change(input, { target: { value: '192.0.2.22' } });
  input.focus();
  await client.invalidateQueries({ queryKey: ['network'] });
  rerender(
    <QueryClientProvider client={client}>
      <NetworkPage canMutate={false} />
    </QueryClientProvider>,
  );
  expect(input).toHaveValue('192.0.2.22');
  expect(input).toHaveFocus();
  expect(input).toBeEnabled();
  expect(screen.getByRole('button', { name: 'Save network settings' })).toBeDisabled();
});
test('tokens can be read during outages and secrets clear on expiry', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(
      async (_url, init) =>
        new Response(JSON.stringify(init?.method === 'POST' ? { token: 'synthetic-once' } : [])),
    ),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <TokensPage canMutate />
    </QueryClientProvider>,
  );
  fireEvent.change(screen.getByLabelText('Token name'), { target: { value: 'test' } });
  fireEvent.click(screen.getByRole('button', { name: 'Create token' }));
  expect(await screen.findByText('synthetic-once')).toBeInTheDocument();
  window.dispatchEvent(new Event('filedock:session-expired'));
  await waitFor(() => expect(screen.queryByText('synthetic-once')).not.toBeInTheDocument());
});

test('recovery downloads a TXT before confirmation and supports expired setup reauthentication', async () => {
  const { RecoveryKeyStep } = await import('../src/features/setup/RecoveryKeyStep.tsx');
  const create = vi.fn().mockReturnValue('blob:synthetic');
  vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: create, revokeObjectURL: vi.fn() }));
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
  const confirm = vi.fn().mockResolvedValue(undefined);
  const { rerender } = render(
    <RecoveryKeyStep recoveryKey="SYNTHETIC-KEY" onDownloaded={confirm} />,
  );
  fireEvent.click(screen.getByRole('button', { name: 'Download recovery key and continue' }));
  await waitFor(() => expect(confirm).toHaveBeenCalledOnce());
  expect(create).toHaveBeenCalled();
  expect(click).toHaveBeenCalledOnce();
  expect(click.mock.instances[0]).toHaveAttribute('download', 'filedock-recovery-key.txt');
  rerender(
    <RecoveryKeyStep recoveryKey="SYNTHETIC-KEY" canContinue={false} onDownloaded={confirm} />,
  );
  fireEvent.click(screen.getByRole('button', { name: 'Download recovery key' }));
  await waitFor(() => expect(click).toHaveBeenCalledTimes(2));
  expect(confirm).toHaveBeenCalledOnce();
  click.mockRestore();
});

test('last enabled administrator is guarded and changing password needs current password', async () => {
  const { AdminsPage } = await import('../src/features/admins/AdminsPage.tsx');
  vi.stubGlobal(
    'fetch',
    vi.fn(
      async (url) =>
        new Response(
          JSON.stringify(
            String(url).endsWith('/auth/me')
              ? { id: 'one', username: 'root' }
              : [{ id: 'one', username: 'root', enabled: true, lockedUntil: null }],
          ),
        ),
    ),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <AdminsPage onSignedOut={() => {}} />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('button', { name: 'Disable root' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Remove root' })).toBeDisabled();
  expect(screen.getByLabelText('Current password')).toBeRequired();
});

test('network disconnect acknowledgement is explicit in submitted payload', async () => {
  const fetcher = vi.fn(
    async (_url, init) =>
      new Response(
        JSON.stringify(init?.method === 'PUT' ? { listenAddresses: ['192.0.2.22'] } : network),
      ),
  );
  vi.stubGlobal('fetch', fetcher);
  render(
    <QueryClientProvider client={new QueryClient()}>
      <NetworkPage />
    </QueryClientProvider>,
  );
  fireEvent.change(await screen.findByLabelText('Listen addresses (one per line)'), {
    target: { value: '192.0.2.22' },
  });
  fireEvent.click(screen.getByLabelText('Allow this change to disconnect my current connection'));
  fireEvent.click(screen.getByRole('button', { name: 'Save network settings' }));
  await waitFor(() =>
    expect(fetcher).toHaveBeenCalledWith(
      '/api/v1/system/network',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({ listenAddresses: ['192.0.2.22'], confirmDisconnect: true }),
      }),
    ),
  );
});
