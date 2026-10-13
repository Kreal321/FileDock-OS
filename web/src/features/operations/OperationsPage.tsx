import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api } from '../../api/client.ts';
import { dataView, displayTime, type OperationPage } from '../../api/view-models.ts';
import { Button } from '../../components/Button.tsx';
import { Switch } from '../../components/Switch.tsx';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { StateView } from '../../components/StateView.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
export function OperationsPage({ connected = true }: { connected?: boolean }) {
  const [activeOnly, setActiveOnly] = useState(false);
  const [tokens, setTokens] = useState<string[]>([]);
  const pageToken = tokens.at(-1);
  const query = useQuery({
    queryKey: ['operations', activeOnly, pageToken],
    queryFn: () =>
      api.get<OperationPage>(
        `/operations?pageSize=25&activeOnly=${activeOnly}${pageToken ? `&pageToken=${encodeURIComponent(pageToken)}` : ''}`,
      ),
    retry: false,
  });
  const view = dataView(
    query.data?.operations,
    query.isPending,
    query.error,
    query.dataUpdatedAt,
    !connected,
  );
  return (
    <div className="stack">
      <PageHeader title="Operations" description="Core operation history and progress." />
      <Panel className="stack">
        <Switch
          label="Active operations only"
          checked={activeOnly}
          onChange={(v) => {
            setActiveOnly(v);
            setTokens([]);
          }}
        />
        <StateView
          {...view}
          onRetry={() => void query.refetch()}
          empty="No operations match this view."
        />
        {query.data?.operations.length ? (
          <DataTable
            label="Operations"
            headers={['Operation', 'State', 'Started by', 'Started', 'Finished']}
          >
            {query.data.operations.map((op) => (
              <tr key={op.id}>
                <td>
                  <a href={`#/settings/operations/${encodeURIComponent(op.id)}`}>{op.kind}</a>
                  <p className="caption muted">{op.id}</p>
                </td>
                <td>
                  <StatusBadge
                    state={op.state}
                    freshness={view.phase === 'stale' ? 'cached' : 'current'}
                  />
                </td>
                <td>{op.startedBy || 'Not reported'}</td>
                <td>{displayTime(op.startedAt)}</td>
                <td>{displayTime(op.finishedAt)}</td>
              </tr>
            ))}
          </DataTable>
        ) : null}
        <div className="actions">
          <Button
            variant="secondary"
            disabled={!tokens.length || query.isFetching}
            onClick={() => setTokens(tokens.slice(0, -1))}
          >
            Previous
          </Button>
          <Button
            variant="secondary"
            disabled={!query.data?.nextPageToken || query.isFetching}
            onClick={() => setTokens([...tokens, query.data!.nextPageToken!])}
          >
            Next
          </Button>
        </div>
      </Panel>
    </div>
  );
}
