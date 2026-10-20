import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { EventsPage } from './EventsPage.tsx';
import { resolveRoute } from '../../navigation/destinations.ts';
test('only the selected record stream fetches, with one heading and three tabs', async () => {
  const fetch = vi
    .fn<(url: string) => Promise<Response>>()
    .mockResolvedValue(new Response(JSON.stringify({ items: [] })));
  vi.stubGlobal('fetch', fetch);
  render(
    <QueryClientProvider client={new QueryClient()}>
      <EventsPage route={resolveRoute('#/settings/activity')} />
    </QueryClientProvider>,
  );
  expect(await screen.findByText('No appliance activity yet.')).toBeVisible();
  expect(screen.getAllByRole('tab')).toHaveLength(3);
  expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
  expect(fetch.mock.calls.every((args) => String(args[0]).includes('/activity?'))).toBe(true);
});
