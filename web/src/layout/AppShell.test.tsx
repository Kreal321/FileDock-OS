import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { AppShell } from './AppShell.tsx';
vi.mock('../api/useLiveCoreStatus.ts', () => ({
  useLiveCoreStatus: () => ({ state: 'Healthy', detail: 'Connected', canMutate: true }),
}));
test('shell uses actual account and native current links; disclosure restores focus', async () => {
  vi.stubGlobal(
    'fetch',
    vi
      .fn()
      .mockImplementation((url: string) =>
        Promise.resolve(
          new Response(
            JSON.stringify(
              url.includes('/alerts') ? { items: [] } : { id: '1', username: 'kyson' },
            ),
          ),
        ),
      ),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <AppShell route={{ id: 'dashboard' }} onSignOut={() => {}}>
        Page
      </AppShell>
    </QueryClientProvider>,
  );
  expect(await screen.findByText('kyson')).toBeInTheDocument();
  expect(screen.getByRole('link', { name: 'Dashboard' })).toHaveAttribute('aria-current', 'page');
  const toggle = screen.getByRole('button', { name: 'Open navigation' });
  fireEvent.click(toggle);
  expect(toggle).toHaveAttribute('aria-expanded', 'true');
  fireEvent.keyDown(screen.getByRole('navigation', { name: 'Main navigation' }), { key: 'Escape' });
  expect(toggle).toHaveAttribute('aria-expanded', 'false');
  expect(toggle).toHaveFocus();
});
