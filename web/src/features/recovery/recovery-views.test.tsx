import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { StateProtectionPage } from './StateProtectionPage.tsx';
import { LocationsPage } from './LocationsPage.tsx';
afterEach(() => vi.unstubAllGlobals());
function mount(component: React.ReactNode) {
  return render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      {component}
    </QueryClientProvider>,
  );
}
test('missing remote coverage is explicit and protected controls obey the mutation gate', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      Response.json({
        remoteDestinationRegistered: false,
        reinstallCovered: false,
        machineLossCovered: false,
        warning: true,
        copies: [
          {
            kind: 'LocalSystemDisk',
            description: 'System disk',
            lastSnapshotAt: '2026-10-13T12:00:00Z',
            stale: false,
            lastError: '',
          },
        ],
      }),
    ),
  );
  mount(<StateProtectionPage canMutate={false} />);
  expect(await screen.findByText(/not covered by a remote copy/)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Snapshot now' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Download configuration backup' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Regenerate recovery key' })).toBeDisabled();
});
test('recovery-reader location keeps its ownership warning and disables takeover when disconnected', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      Response.json({
        locations: [
          {
            connectionId: 'c',
            bucket: 'bucket',
            prefix: 'state',
            purpose: 'ApplianceState',
            ownerApplianceId: 'owner',
            generation: 2,
            role: 'RecoveryReader',
            servedByConnector: 'rclone',
            claimSafety: 'Basic',
          },
        ],
      }),
    ),
  );
  mount(<LocationsPage canMutate={false} />);
  expect(await screen.findByText(/RecoveryReader/)).toBeInTheDocument();
  expect(screen.getByText(/Basic claim safety/)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Take over location' })).toBeDisabled();
});
