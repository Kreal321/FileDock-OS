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
  expect(screen.queryByLabelText('Extension package (.fdext)')).not.toBeInTheDocument();
  expect(screen.getByRole('link', { name: 'Install extension' })).toHaveAttribute(
    'href',
    '#/extensions/package',
  );
});
test('package review retains unsigned and permission consent before installation', async () => {
  const { ExtensionPackagePage } = await import('./ExtensionPackagePage.tsx');
  const { fireEvent } = await import('@testing-library/react');
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: string) =>
      Response.json(
        input.includes('/inspect')
          ? {
              stagedPackageId: 'stage',
              valid: true,
              signed: false,
              signerKeyId: '',
              manifest: { id: 'test', name: 'Synthetic package', version: '1' },
              requestedPermissions: ['network.outbound'],
              issues: [],
              dataPaths: [],
            }
          : input.includes('/folders')
            ? { folders: [] }
            : { subsystems: [], defaultProviders: {} },
      ),
    ),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ExtensionPackagePage />
    </QueryClientProvider>,
  );
  fireEvent.change(screen.getByLabelText('Extension package (.fdext)'), {
    target: { files: [new File(['synthetic'], 'test.fdext')] },
  });
  const approve = await screen.findByRole('button', { name: 'Approve and install' });
  expect(approve).toBeDisabled();
  fireEvent.click(screen.getByLabelText('network.outbound'));
  expect(approve).toBeDisabled();
  fireEvent.click(screen.getByLabelText('I accept installing an unsigned package.'));
  expect(approve).toBeEnabled();
});
