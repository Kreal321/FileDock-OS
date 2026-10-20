import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { FolderEditorPage } from './FolderEditorPage.tsx';
import { folderOptionsFixture, layoutStates } from '../../../tests/fixtures/ui-states.ts';
test('edit loads exact ID and retains the draft after generation conflict', async () => {
  const fetch = vi.fn(async (input: string, init?: RequestInit) => {
    if (input.includes('/options')) return Response.json(folderOptionsFixture);
    if (input.includes('/validate')) return Response.json({ valid: true, issues: [] });
    if (init?.method === 'PATCH')
      return new Response(JSON.stringify({ detail: 'Generation changed' }), { status: 409 });
    return Response.json(layoutStates.folders[0]);
  });
  vi.stubGlobal('fetch', fetch);
  render(
    <QueryClientProvider client={new QueryClient()}>
      <FolderEditorPage id="folder one" canMutate />
    </QueryClientProvider>,
  );
  const name = await screen.findByLabelText('Name');
  fireEvent.change(name, { target: { value: 'New Name' } });
  expect(screen.getByLabelText('FileDock Path')).toHaveValue('/storage/new-name');
  fireEvent.click(screen.getByRole('button', { name: 'Save folder' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Generation changed');
  expect(name).toHaveValue('New Name');
  expect(fetch.mock.calls.some(([url]) => url.endsWith('/folders/folder%20one'))).toBe(true);
});
