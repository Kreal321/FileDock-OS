import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { ConnectorsPage } from './ConnectorsPage.tsx';
const defaults = [
  {
    family: 's3-compatible',
    use: 'OwnedObjectStorage',
    connectionId: '',
    target: { connectorId: 'fd-s3' },
  },
];
function setup(canMutate = true) {
  const fetcher = vi.fn(
    async (url, init) =>
      new Response(
        JSON.stringify(
          String(url).endsWith('/preview')
            ? {
                requiresBasicClaimAck: true,
                requiresThirdPartyOwnedStorageAck: true,
                connectionIds: ['one'],
                credentialRecipients: { sample: { connectionIds: ['one'] } },
              }
            : String(url).endsWith('/connector-routes')
              ? { generation: 4, defaults, overrides: [] }
              : String(url).includes('/connections')
                ? {
                    items: [
                      { id: 'one', spec: { name: 'Archive', providerFamily: 's3-compatible' } },
                    ],
                  }
                : {
                    connectors: [
                      {
                        subsystemId: 'fd-s3',
                        enabled: true,
                        class: 'BuiltIn',
                        state: 'Running',
                        families: [
                          {
                            family: 's3-compatible',
                            capabilities: ['cloud.remote'],
                            conditionalWrites: true,
                          },
                        ],
                      },
                      {
                        subsystemId: 'sample',
                        enabled: true,
                        class: 'Extension',
                        state: 'Running',
                        families: [{ family: 's3-compatible', capabilities: ['cloud.remote'] }],
                      },
                    ],
                  },
        ),
        { status: init?.method === 'PUT' ? 200 : 200 },
      ),
  );
  vi.stubGlobal('fetch', fetcher);
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <ConnectorsPage canMutate={canMutate} />
    </QueryClientProvider>,
  );
  return fetcher;
}
test('route preview requires both acknowledgements and submits the preview generation', async () => {
  const fetcher = setup();
  fireEvent.click(
    await screen.findByRole('button', { name: 'Actions for s3-compatible owned storage default' }),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Change connector' }));
  fireEvent.change(screen.getByLabelText('Connector'), { target: { value: 'sample' } });
  fireEvent.click(screen.getByRole('button', { name: 'Preview change' }));
  expect(await screen.findByText('Archive')).toBeInTheDocument();
  const apply = screen.getByRole('button', { name: 'Apply route change' });
  expect(apply).toBeDisabled();
  fireEvent.click(screen.getByLabelText(/I accept basic claim safety/));
  expect(apply).toBeDisabled();
  fireEvent.click(screen.getByLabelText(/I trust this third-party connector/));
  fireEvent.click(apply);
  await waitFor(() =>
    expect(fetcher).toHaveBeenCalledWith(
      '/api/v1/connector-routes',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({
          changes: [
            {
              family: 's3-compatible',
              use: 'OwnedObjectStorage',
              connectionId: '',
              target: { connectorId: 'sample' },
            },
          ],
          expectedGeneration: 4,
          acknowledgedBasicClaimSafety: true,
          acknowledgedThirdPartyOwnedStorage: true,
        }),
      }),
    ),
  );
});
test('read-only administrators can inspect routes but cannot change them', async () => {
  setup(false);
  expect(
    await screen.findByRole('button', { name: 'Actions for s3-compatible owned storage default' }),
  ).toBeInTheDocument();
  fireEvent.click(
    screen.getByRole('button', { name: 'Actions for s3-compatible owned storage default' }),
  );
  expect(screen.getByRole('button', { name: 'Change connector' })).toBeDisabled();
  expect(screen.getByRole('table', { name: 'Family defaults' })).toBeInTheDocument();
});

test('failed apply preserves the chosen connector and acknowledgements', async () => {
  const fetcher = setup();
  const original = fetcher.getMockImplementation()!;
  fetcher.mockImplementation(async (url, init) =>
    init?.method === 'PUT'
      ? new Response(JSON.stringify({ detail: 'route.generation_mismatch' }), { status: 409 })
      : original(url, init),
  );
  fireEvent.click(
    await screen.findByRole('button', { name: 'Actions for s3-compatible owned storage default' }),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Change connector' }));
  fireEvent.change(screen.getByLabelText('Connector'), { target: { value: 'sample' } });
  fireEvent.click(screen.getByRole('button', { name: 'Preview change' }));
  fireEvent.click(await screen.findByLabelText(/I accept basic claim safety/));
  fireEvent.click(screen.getByLabelText(/I trust this third-party connector/));
  fireEvent.click(screen.getByRole('button', { name: 'Apply route change' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('route.generation_mismatch');
  expect(screen.getByRole('dialog')).toBeInTheDocument();
  expect(screen.getByLabelText('Connector')).toHaveValue('sample');
  expect(screen.getByLabelText(/I accept basic claim safety/)).toBeChecked();
});
