import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, expect, test, vi } from 'vitest';
import { api } from '../../api/client.ts';
import { SharesPage } from './SharesPage.tsx';
vi.mock('../../api/client.ts', () => ({ api: { get: vi.fn(), post: vi.fn(), put: vi.fn() } }));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
test('share account creation uses a focused dialog and no permanent password form', async () => {
  vi.mocked(api.get).mockResolvedValue({ items: [] });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <SharesPage />
    </QueryClientProvider>,
  );
  await screen.findByText(/No share users yet/);
  expect(screen.queryByLabelText('Password')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Add share user' }));
  expect(screen.getByRole('dialog', { name: 'Add share user' })).toBeInTheDocument();
  expect(screen.getByLabelText('Password')).toHaveAttribute('type', 'password');
});
