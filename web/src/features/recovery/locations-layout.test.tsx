import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { LocationsPage } from './LocationsPage.tsx';
import { resolveRoute } from '../../navigation/destinations.ts';
test('Cloud owns locations and takeover retains observed generation and recent writer acknowledgement', async () => {
  const fetch = vi.fn(async () =>
    Response.json({
      locations: [
        {
          connectionId: 'c',
          bucket: 'archive',
          prefix: 'state',
          purpose: 'ApplianceState',
          role: 'RecoveryReader',
          ownerApplianceId: 'old',
          generation: '7',
          servedByConnector: 'fd-s3',
          claimSafety: 'Conditional',
        },
      ],
    }),
  );
  vi.stubGlobal('fetch', fetch);
  vi.spyOn(window, 'confirm').mockReturnValue(true);
  render(
    <QueryClientProvider client={new QueryClient()}>
      <LocationsPage />
    </QueryClientProvider>,
  );
  fireEvent.click(await screen.findByRole('button', { name: 'Take over location' }));
  await vi.waitFor(() => expect(fetch).toHaveBeenCalledTimes(3));
  const mutation = (fetch.mock.calls as unknown as [string, RequestInit][]).find(
    ([, init]) => init?.method === 'POST',
  )!;
  expect(JSON.parse(String(mutation[1].body))).toMatchObject({
    observedGeneration: '7',
    acknowledgedRecentWriter: true,
  });
  expect(resolveRoute('#/settings/locations')).toMatchObject({
    owner: 'locations',
    groupId: 'cloud-group',
  });
  vi.restoreAllMocks();
});
