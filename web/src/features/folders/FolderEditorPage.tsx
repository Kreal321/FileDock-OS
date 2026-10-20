import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api } from '../../api/client.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { StateView } from '../../components/StateView.tsx';
import { FolderForm, type Folder, type Options } from './FolderForm.tsx';
export function FolderEditorPage({
  id,
  canMutate = false,
  connected = true,
}: {
  id?: string;
  canMutate?: boolean;
  connected?: boolean;
}) {
  const cache = useQueryClient();
  const options = useQuery({
    queryKey: ['folder-options'],
    queryFn: () => api.get<Options>('/folders/options'),
  });
  const folder = useQuery({
    queryKey: ['folders', 'detail', id],
    queryFn: () => api.get<Folder>(`/folders/${encodeURIComponent(id!)}`),
    enabled: !!id,
  });
  return (
    <section className="stack">
      <PageHeader
        title={id ? 'Edit Managed Folder' : 'Create Managed Folder'}
        description="Configure one folder in six sections. Review its protection and potential loss before saving."
      />
      {options.error || (id && folder.error) ? (
        <StateView
          phase="unavailable"
          error={options.error?.message ?? folder.error?.message}
          onRetry={() => {
            void options.refetch();
            if (id) void folder.refetch();
          }}
        />
      ) : null}
      {options.data && (!id || folder.data) ? (
        <FolderForm
          key={id ?? 'new'}
          folder={folder.data}
          options={options.data}
          canMutate={canMutate && connected && !options.error && !folder.error}
          onSaved={() => {
            void cache.invalidateQueries({ queryKey: ['folders'] });
            void cache.invalidateQueries({ queryKey: ['extension-folders'] });
            window.location.hash = '#/folders';
          }}
          onCancel={() => {
            window.location.hash = '#/folders';
          }}
        />
      ) : !options.error && !folder.error ? (
        <p role="status">Loading folder settings…</p>
      ) : null}
    </section>
  );
}
