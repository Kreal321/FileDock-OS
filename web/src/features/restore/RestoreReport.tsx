import { DataTable } from '../../components/DataTable.tsx';
import { useQuery } from '@tanstack/react-query';
import { api } from '../../api/client.ts';
import { Panel } from '../../components/Panel.tsx';
import { Notice } from '../../components/Notice.tsx';
import { Button } from '../../components/Button.tsx';
export function RestoreReport() {
  const q = useQuery({
    queryKey: ['restore-report'],
    queryFn: () =>
      api
        .get<{
          restoreOperationId: string;
          snapshotTakenAt: string;
          folders?: { folderId: string; name: string; outcome: string; detail: string }[];
          items?: { kind: string; id: string; name: string; status: string; detail: string }[];
        }>('/restore/report')
        .then((report) => report ?? null),
  });
  if (!q.data) return null;
  return (
    <Panel className="stack">
      <h2>Restore report</h2>
      <Notice>
        Recovered snapshot: {q.data.snapshotTakenAt}. Later changes are lost. Review every missing
        folder and pending component.
      </Notice>
      {q.data.folders
        ?.filter((f) => f.outcome === 'LOST' || f.outcome === 'TO_REBUILD')
        .map((f) => (
          <Notice tone="danger" key={`loss:${f.folderId}`}>
            {f.name}: {f.outcome}. {f.detail}
          </Notice>
        ))}
      {q.data.folders?.length ? (
        <DataTable label="Recovered folders" headers={['Folder', 'Outcome', 'Detail']}>
          {q.data.folders.map((f) => (
            <tr key={f.folderId}>
              <td>{f.name}</td>
              <td>{f.outcome}</td>
              <td>{f.detail}</td>
            </tr>
          ))}
        </DataTable>
      ) : null}
      {q.data.items?.length ? (
        <DataTable label="Recovered components" headers={['Kind', 'Name', 'Status', 'Detail']}>
          {q.data.items.map((i) => (
            <tr key={`${i.kind}:${i.id}`}>
              <td>{i.kind}</td>
              <td>{i.name || i.id}</td>
              <td>{i.status}</td>
              <td>{i.detail}</td>
            </tr>
          ))}
        </DataTable>
      ) : null}
      <a href={`#/settings/operations/${encodeURIComponent(q.data.restoreOperationId)}`}>
        View restore operation
      </a>
      <Button onClick={() => void api.post('/restore/report/dismiss').then(() => q.refetch())}>
        Dismiss this report
      </Button>
    </Panel>
  );
}
