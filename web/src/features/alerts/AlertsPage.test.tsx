import { render, screen, waitFor, cleanup } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, expect, it, vi } from 'vitest';
import { CriticalAlertBanner } from './AlertsPage.tsx';
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
it('keeps active critical conditions visible across pages', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          items: [
            {
              id: 'critical',
              severity: 'Critical',
              message: 'Pool is unavailable',
              state: 'Active',
            },
          ],
        }),
        { status: 200 },
      ),
    ),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <CriticalAlertBanner />
    </QueryClientProvider>,
  );
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Pool is unavailable'));
  expect(screen.getByRole('link', { name: 'Review alerts' })).toHaveAttribute(
    'href',
    '#/settings/alerts',
  );
});
