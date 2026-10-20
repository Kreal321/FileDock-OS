import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { AccessSecurityPage } from './AccessSecurityPage.tsx';
test('one header, requested token focus and independent account error', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: string) =>
      input.endsWith('/admins')
        ? new Response(JSON.stringify({ detail: 'Accounts unavailable' }), { status: 503 })
        : Response.json(input.endsWith('/tokens') ? [] : { id: 'root', username: 'root' }),
    ),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <AccessSecurityPage section="tokens" onSignedOut={() => {}} />
    </QueryClientProvider>,
  );
  expect(await screen.findByText('Accounts unavailable', { exact: false })).toBeVisible();
  expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
  expect(screen.getByRole('heading', { name: 'API tokens' })).toHaveFocus();
  fireEvent.click(screen.getByRole('button', { name: /^Create token$/ }));
  expect(screen.getByRole('dialog', { name: 'Create token' })).toBeVisible();
  expect(screen.queryByText('Share users')).not.toBeInTheDocument();
});
