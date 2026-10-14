import { useQuery } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { api } from '../../api/client.ts';
import { dataView, displayTime, type ActivityPage } from '../../api/view-models.ts';
import { Button } from '../../components/Button.tsx';
import { Field } from '../../components/Field.tsx';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { StateView } from '../../components/StateView.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
export function AuditPage({ connected = true }: { connected?: boolean }) {
  const [filters, setFilters] = useState({
    actor: '',
    action: '',
    target: '',
    result: '',
    since: '',
    until: '',
  });
  const [tokens, setTokens] = useState<string[]>([]);
  const pageToken = tokens.at(-1);
  const params = new URLSearchParams({
    pageSize: '25',
    ...filters,
    ...(pageToken ? { pageToken } : {}),
  });
  const q = useQuery({
    queryKey: ['audit', filters, pageToken],
    queryFn: () => api.get<ActivityPage>(`/audit?${params}`),
    retry: false,
  });
  const view = dataView(q.data?.items, q.isPending, q.error, q.dataUpdatedAt, !connected);
  function filter(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = new FormData(e.currentTarget);
    setFilters({
      actor: String(form.get('actor') ?? ''),
      action: String(form.get('action') ?? ''),
      target: String(form.get('target') ?? ''),
      result: String(form.get('result') ?? ''),
      since: String(form.get('since') ?? ''),
      until: String(form.get('until') ?? ''),
    });
    setTokens([]);
  }
  return (
    <div className="stack">
      <PageHeader
        title="Audit"
        description="Administrative activity, newest first. This is not file activity."
      />
      <Panel className="stack">
        <form className="form-grid" onSubmit={filter}>
          <Field label="Actor" name="actor" />
          <Field label="Action" name="action" />
          <Field label="Target" name="target" />
          <Field label="Result" name="result" />
          <Field label="Since (ISO timestamp)" name="since" />
          <Field label="Until (ISO timestamp)" name="until" />
          <Button type="submit" variant="secondary">
            Apply filters
          </Button>
        </form>
        <StateView
          {...view}
          onRetry={() => void q.refetch()}
          empty="No administrative activity matches these filters."
        />
        {q.data?.items.length ? (
          <DataTable
            label="Administrative activity"
            headers={['Time', 'Actor', 'Action', 'Target', 'Result']}
          >
            {q.data.items.map((item) => (
              <tr key={item.id}>
                <td>{displayTime(item.at)}</td>
                <td>{item.actorName}</td>
                <td>{item.action}</td>
                <td>{item.target}</td>
                <td>
                  <StatusBadge state={item.result} />
                </td>
              </tr>
            ))}
          </DataTable>
        ) : null}
        <div className="actions">
          <Button
            variant="secondary"
            disabled={!tokens.length || q.isFetching}
            onClick={() => setTokens(tokens.slice(0, -1))}
          >
            Previous
          </Button>
          <Button
            variant="secondary"
            disabled={!q.data?.nextPageToken || q.isFetching}
            onClick={() => setTokens([...tokens, q.data!.nextPageToken!])}
          >
            Next
          </Button>
        </div>
      </Panel>
    </div>
  );
}
