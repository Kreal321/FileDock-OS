import { useQuery } from '@tanstack/react-query';
import { api } from '../../api/client.ts';
import { dataView, displayTime, type OperationView } from '../../api/view-models.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { Notice } from '../../components/Notice.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { StateView } from '../../components/StateView.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
export function OperationDetailPage({ id, connected = true }: { id: string; connected?: boolean }) {
  const q = useQuery({
    queryKey: ['operations', id],
    queryFn: () => api.get<OperationView>(`/operations/${encodeURIComponent(id)}`),
    retry: false,
  });
  const view = dataView(q.data, q.isPending, q.error, q.dataUpdatedAt, !connected);
  return (
    <div className="stack">
      <PageHeader title="Operation detail" description={id} />
      <a href="#/settings/operations">Back to operations</a>
      <StateView {...view} onRetry={() => void q.refetch()} />
      {q.data ? (
        <Panel className="stack">
          <h2>{q.data.kind}</h2>
          <StatusBadge
            state={q.data.state}
            freshness={view.phase === 'stale' ? 'cached' : 'current'}
          />
          <dl>
            <dt>Started by</dt>
            <dd>{q.data.startedBy || 'Not reported'}</dd>
            <dt>Started</dt>
            <dd>{displayTime(q.data.startedAt)}</dd>
            <dt>Finished</dt>
            <dd>{displayTime(q.data.finishedAt)}</dd>
          </dl>
          {q.data.error ? <Notice tone="danger">{q.data.error}</Notice> : null}
          {q.data.steps.length ? (
            <DataTable label="Operation steps" headers={['Step', 'State', 'Attempts', 'Detail']}>
              {q.data.steps.map((step, i) => (
                <tr key={i}>
                  <td>{step.name}</td>
                  <td>
                    <StatusBadge state={step.state} />
                  </td>
                  <td>{step.attempts}</td>
                  <td>{step.detail}</td>
                </tr>
              ))}
            </DataTable>
          ) : (
            <p>No steps reported.</p>
          )}
        </Panel>
      ) : null}
    </div>
  );
}
