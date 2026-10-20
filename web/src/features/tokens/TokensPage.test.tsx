import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { TokensSection } from './TokensPage.tsx';

test('shows a token once and removes the secret when dismissed', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(
      async (_url: string, init?: RequestInit) =>
        new Response(
          JSON.stringify(init?.method === 'POST' ? { id: 'one', token: 'fdt_one_secret' } : []),
          { headers: { 'content-type': 'application/json' } },
        ),
    ),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <TokensSection />
    </QueryClientProvider>,
  );
  fireEvent.click(screen.getByRole('button', { name: 'Create token' }));
  fireEvent.change(screen.getByLabelText('Token name'), { target: { value: 'backup' } });
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Create token' }));
  expect(await screen.findByText('fdt_one_secret')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'I saved the token' }));
  await waitFor(() => expect(screen.queryByText('fdt_one_secret')).not.toBeInTheDocument());
});
