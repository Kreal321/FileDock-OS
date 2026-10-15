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
      {q.data.folders?.map((f) => (
        <p key={f.folderId}>
          <strong>
            {f.name}: {f.outcome}
          </strong>{' '}
          — {f.detail}
        </p>
      ))}
      {q.data.items?.map((i) => (
        <p key={`${i.kind}:${i.id}`}>
          {i.kind} {i.name || i.id}: {i.status}. {i.detail}
        </p>
      ))}
      <a href={`#/settings/operations/${q.data.restoreOperationId}`}>View restore operation</a>
      <Button onClick={() => void api.post('/restore/report/dismiss').then(() => q.refetch())}>
        Dismiss this report
      </Button>
    </Panel>
  );
}
