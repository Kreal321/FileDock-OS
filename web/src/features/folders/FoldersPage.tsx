import { RecoveryPoint } from '../../components/RecoveryPoint.tsx';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { api } from '../../api/client.ts';
import { dataView } from '../../api/view-models.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { StateView } from '../../components/StateView.tsx';
import { Notice } from '../../components/Notice.tsx';
import { Button } from '../../components/Button.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
import { Field } from '../../components/Field.tsx';
import { FolderForm, type Folder, type Options } from './FolderForm.tsx';
const modeNames = {
  LocalOnly: 'Local Only',
  CloudMount: 'Cloud Mount',
  HybridBackup: 'Hybrid Backup',
  TieredStorage: 'Tiered Storage',
};
export function FoldersPage({
  canMutate = false,
  connected = true,
}: {
  canMutate?: boolean;
  connected?: boolean;
}) {
  const cache = useQueryClient();
  const [editing, setEditing] = useState<Folder | null | undefined>();
  const [removing, setRemoving] = useState<Folder>();
  const [typed, setTyped] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [pages, setPages] = useState<string[]>([]);
  const q = useQuery({
    queryKey: ['folders', pages.at(-1)],
    queryFn: () =>
      api.get<{ items: Folder[]; nextPageToken?: string }>(
        `/folders?pageSize=50&pageToken=${encodeURIComponent(pages.at(-1) ?? '')}`,
      ),
    retry: false,
  });
  const options = useQuery({
    queryKey: ['folder-options'],
    queryFn: () => api.get<Options>('/folders/options'),
    retry: false,
  });
  async function refresh() {
    await cache.invalidateQueries({ queryKey: ['folders'] });
    await cache.invalidateQueries({ queryKey: ['extensions-folders'] });
    setEditing(undefined);
  }
  async function remove() {
    if (!removing) return;
    setBusy(true);
    setError('');
    try {
      await api.del(
        `/folders/${removing.id}?typedName=${encodeURIComponent(typed)}`,
        String(removing.generation),
      );
      setRemoving(undefined);
      await refresh();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="stack">
      <PageHeader
        title={
          editing !== undefined
            ? editing
              ? 'Edit Managed Folder'
              : 'Create Managed Folder'
            : 'Managed Folders'
        }
        description="Storage mode defines where data lives. Every folder shows its protection and potential loss."
      />
      {error ? <Notice tone="danger">{error}</Notice> : null}
      {editing !== undefined && options.data ? (
        <FolderForm
          key={editing?.id ?? 'new'}
          folder={editing ?? undefined}
          options={options.data}
          canMutate={canMutate && connected && !options.error}
          onSaved={() => void refresh()}
          onCancel={() => setEditing(undefined)}
        />
      ) : (
        <Panel className="stack">
          <Button
            disabled={!canMutate || !connected || !options.data || !!q.error || !!options.error}
            onClick={() => setEditing(null)}
          >
            Create Managed Folder
          </Button>
          {options.error ? (
            <Notice tone="danger">Folder options unavailable. {options.error.message}</Notice>
          ) : null}
          <StateView
            {...dataView(q.data?.items, q.isPending, q.error, q.dataUpdatedAt, !connected)}
            onRetry={() => void q.refetch()}
            empty="No managed folders yet."
          />
          {q.data?.items.length ? (
            <DataTable
              label="Managed folders"
              headers={[
                'Name / Path',
                'Storage mode',
                'Data importance',
                'Access',
                'Protection / Potential loss',
                'Status',
                'Actions',
              ]}
            >
              {q.data.items.map((f) => (
                <tr key={f.id}>
                  <td>
                    <strong>{f.identity.name}</strong>
                    <p>{f.identity.fileDockPath}</p>
                  </td>
                  <td>{modeNames[f.storageMode]}</td>
                  <td>{f.storageMode === 'HybridBackup' ? f.dataImportance : '—'}</td>
                  <td>{f.access.join(', ') || 'None'}</td>
                  <td>
                    <StatusBadge state={f.protectionState} />
                    <p>{f.exposure}</p>
                    <RecoveryPoint
                      label="Durable folder-data recovery point"
                      at={f.status?.lastDurableRecoveryPoint}
                      cached={!connected || Boolean(q.error)}
                    />
                  </td>
                  <td>
                    {f.status?.state ?? 'Pending'}
                    <p className="muted">{f.status?.detail}</p>
                  </td>
                  <td>
                    <div className="actions">
                      <Button
                        variant="secondary"
                        disabled={!canMutate || !connected || !options.data || !!q.error}
                        onClick={() => setEditing(f)}
                      >
                        Edit
                      </Button>
                      <Button
                        variant="danger"
                        disabled={!canMutate || !connected || !!q.error}
                        onClick={() => {
                          setRemoving(f);
                          setTyped('');
                        }}
                      >
                        Delete
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </DataTable>
          ) : null}
          <div className="actions">
            <Button
              variant="secondary"
              disabled={!pages.length || q.isFetching}
              onClick={() => setPages(pages.slice(0, -1))}
            >
              Previous
            </Button>
            <Button
              variant="secondary"
              disabled={!q.data?.nextPageToken || q.isFetching}
              onClick={() => setPages([...pages, q.data!.nextPageToken!])}
            >
              Next
            </Button>
          </div>
        </Panel>
      )}
      <Dialog
        open={!!removing}
        title="Delete managed folder"
        onClose={() => setRemoving(undefined)}
        busy={busy}
      >
        <p>
          Remove {removing?.identity.name} from FileDock? The underlying pool data is kept. Exports,
          apps and extension bindings must be removed first.
        </p>
        {removing?.dataImportance === 'Critical' ? (
          <Field
            label="Type the folder name"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
          />
        ) : null}
        <Button
          variant="danger"
          disabled={
            !canMutate ||
            !connected ||
            busy ||
            (removing?.dataImportance === 'Critical' && typed !== removing.identity.name)
          }
          busy={busy}
          onClick={() => void remove()}
        >
          Delete folder
        </Button>
      </Dialog>
    </div>
  );
}
