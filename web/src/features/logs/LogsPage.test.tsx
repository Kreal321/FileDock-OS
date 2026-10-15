import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { LogsPage } from './LogsPage.tsx';
import { ActivityPage } from '../activity/ActivityPage.tsx';
const renderPage = (page: React.ReactNode) =>
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      {page}
    </QueryClientProvider>,
  );
test('long log preview opens complete returned message in a named dialog', async () => {
  const message = 'fail: restore.target_not_fresh ' + 'stack trace details '.repeat(100);
  vi.stubGlobal(
    'fetch',
    vi.fn(
      async () =>
        new Response(
          JSON.stringify({
            items: [
              {
                at: '2026-10-15T12:00:00Z',
                source: 'filedock-management.service',
                severity: 'Error',
                message,
              },
            ],
          }),
        ),
    ),
  );
  renderPage(<LogsPage />);
  const trigger = await screen.findByRole('button', {
    name: /View log details from filedock-management/,
  });
  expect(trigger.querySelector('.log-message-preview')).toHaveTextContent(message.trim());
  trigger.focus();
  fireEvent.click(trigger);
  const dialog = screen.getByRole('dialog', { name: 'Log details' });
  expect(within(dialog).getByLabelText('Full log message')).toHaveTextContent(message.trim());
  expect(within(dialog).getByText('filedock-management.service')).toBeVisible();
  fireEvent.click(within(dialog).getByRole('button', { name: 'Close', exact: true }));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
});
test('legacy identical activity coalesces but distinct protection changes remain', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(
      async () =>
        new Response(
          JSON.stringify({
            items: [
              {
                seq: 3,
                at: '2026-10-15T12:00:04Z',
                type: 'state.protection.changed',
                payload: { warning: true },
              },
              {
                seq: 2,
                at: '2026-10-15T12:00:02Z',
                type: 'state.protection.changed',
                payload: { warning: true },
              },
              {
                seq: 1,
                at: '2026-10-15T12:00:00Z',
                type: 'state.protection.changed',
                payload: { warning: false },
              },
            ],
          }),
        ),
    ),
  );
  renderPage(<ActivityPage />);
  expect(await screen.findAllByText('Recovery protection updated')).toHaveLength(2);
  expect(screen.getByText(/2 identical updates/)).toBeVisible();
});
