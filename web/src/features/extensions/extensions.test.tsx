import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { ExtensionsPage } from './ExtensionsPage.tsx';
afterEach(() => vi.unstubAllGlobals());
test('built-ins cannot be removed and offline mutation controls are disabled', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input) =>
      Response.json(
        String(input).includes('folders')
          ? { folders: [] }
          : {
              subsystems: [
                {
                  subsystemId: 'docker',
                  name: 'Docker',
                  version: '1.0.0',
                  class: 'BuiltIn',
                  runtimeType: 'systemd-service',
                  state: 'Running',
                  capabilities: ['container.runtime'],
                  dependentFolders: [],
                  dataBindings: {},
                  dataProtected: true,
                },
              ],
              defaultProviders: {},
            },
      ),
    ),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <ExtensionsPage canMutate={false} />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('heading', { name: /Docker/ })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /^Remove$/ })).not.toBeInTheDocument();
  expect(screen.getByLabelText('Extension package (.fdext)')).toBeDisabled();
});
