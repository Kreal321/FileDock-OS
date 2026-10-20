import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { AlertsDeliveryPage } from './AlertsDeliveryPage.tsx';
test('one combined heading, real delivery test failure is not success', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: string) => {
      if (input.endsWith('/test'))
        return new Response(JSON.stringify({ detail: 'Delivery failed' }), { status: 502 });
      if (input.endsWith('/notification-channels'))
        return Response.json({
          items: [
            {
              id: 'c',
              name: 'Recorder',
              subsystemId: 'notify',
              minSeverity: 'Warning',
              config: {},
              secretRefs: [],
            },
          ],
        });
      if (input.endsWith('/subsystems'))
        return Response.json({
          subsystems: [
            { subsystemId: 'notify', name: 'Recorder provider', capabilities: ['notify.channel'] },
          ],
        });
      return Response.json({ items: [] });
    }),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <AlertsDeliveryPage canMutate section="delivery" />
    </QueryClientProvider>,
  );
  expect(await screen.findByText('Recorder', { exact: true })).toBeVisible();
  expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
  fireEvent.click(screen.getByRole('button', { name: 'Actions for Recorder' }));
  fireEvent.click(screen.getByRole('button', { name: 'Send test to Recorder' }));
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: /^Send test$/ }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Delivery failed');
  expect(screen.queryByText(/Test delivered/)).not.toBeInTheDocument();
});
