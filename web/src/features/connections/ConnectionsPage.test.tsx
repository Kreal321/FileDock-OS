import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, expect, test, vi } from 'vitest';
import { api } from '../../api/client.ts';
import { ConnectionsPage } from './ConnectionsPage.tsx';
vi.mock('../../api/client.ts', () => ({ api: { get: vi.fn(), post: vi.fn() } }));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
test('bound providers render the advertised connector schema and keep tokens out of the regular form', async () => {
  vi.mocked(api.get).mockImplementation(async (path) =>
    path === '/connections/options'
      ? ({
          connectors: [
            {
              subsystemId: 'rclone',
              enabled: true,
              families: [
                {
                  family: 'google-drive',
                  connectionSchema: {
                    properties: { scope: { type: 'string', title: 'Drive scope' } },
                  },
                },
              ],
            },
          ],
        } as never)
      : ({ items: [] } as never),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ConnectionsPage />
    </QueryClientProvider>,
  );
  await screen.findByText('No cloud connections yet.');
  fireEvent.click(screen.getByRole('button', { name: 'Add connection' }));
  fireEvent.change(screen.getByLabelText('Provider family'), { target: { value: 'google-drive' } });
  expect(screen.getByLabelText('Drive scope')).toBeInTheDocument();
  expect(screen.queryByLabelText('Token')).not.toBeInTheDocument();
});
