import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { OperationsPage } from './OperationsPage.tsx';
import { operation } from '../../../tests/fixtures/ui-states.ts';
test('Jobs preserve real operation identity and opaque continuation', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      Response.json({ operations: [{ ...operation, id: 'job / one' }], nextPageToken: 'opaque' }),
    ),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <OperationsPage />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('link', { name: 'ReconcileAll' })).toHaveAttribute(
    'href',
    '#/settings/operations/job%20%2F%20one',
  );
  expect(screen.getByRole('button', { name: 'Next' })).toBeEnabled();
});
