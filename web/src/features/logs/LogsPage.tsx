import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api } from '../../api/client.ts';
import { dataView, displayTime } from '../../api/view-models.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { Field } from '../../components/Field.tsx';
import { Select } from '../../components/Select.tsx';
import { Button } from '../../components/Button.tsx';
import { StateView } from '../../components/StateView.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
import { DataTable } from '../../components/DataTable.tsx';
type Log = { at: string; source: string; severity: string; message: string };
export function LogsPage({
  connected = true,
  canMutate = false,
}: {
  connected?: boolean;
  canMutate?: boolean;
}) {
  const [filters, setFilters] = useState({
    source: '',
    minSeverity: 'Info',
    since: '',
    until: '',
    text: '',
  });
  const [pages, setPages] = useState<string[]>([]);
  const params = new URLSearchParams({
    ...Object.fromEntries(Object.entries(filters).filter(([, v]) => v)),
    pageSize: '100',
    ...(pages.length ? { pageToken: pages.at(-1)! } : {}),
  });
  const q = useQuery({
    queryKey: ['logs', filters, pages.at(-1)],
    queryFn: () => api.get<{ items: Log[]; nextPageToken?: string }>(`/logs?${params}`),
    retry: false,
  });
  return (
    <div className="stack">
      <PageHeader
        title="Logs"
        description="Operational logs from the appliance, newest first. Journal retention does not remove audit history."
      />
      <Panel className="stack">
        <form
          className="form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            setFilters({
              source: String(f.get('source')),
              minSeverity: String(f.get('minSeverity')),
              since: String(f.get('since')),
              until: String(f.get('until')),
              text: String(f.get('text')),
            });
            setPages([]);
          }}
        >
          <Field label="Source unit" name="source" placeholder="filedock-core.service" />
          <Select label="Minimum severity" name="minSeverity" defaultValue="Info">
            {['Debug', 'Info', 'Warning', 'Error', 'Critical'].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </Select>
          <Field label="Since (ISO timestamp)" name="since" />
          <Field label="Until (ISO timestamp)" name="until" />
          <Field label="Containing text" name="text" />
          <Button type="submit">Apply filters</Button>
        </form>
        <StateView
          {...dataView(q.data?.items, q.isPending, q.error, q.dataUpdatedAt, !connected)}
          onRetry={() => void q.refetch()}
          empty="No journal entries match these filters."
        />
        {q.data?.items.length ? (
          <DataTable label="Journal entries" headers={['Time', 'Source', 'Severity', 'Message']}>
            {q.data.items.map((l, i) => (
              <tr key={i}>
                <td>{displayTime(l.at)}</td>
                <td>{l.source}</td>
                <td>
                  <StatusBadge state={l.severity} />
                </td>
                <td className="wrap-anywhere">{l.message}</td>
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
          {canMutate && connected ? (
            <a className="button button-secondary" href="/api/v1/diagnostics/bundle" download>
              Download diagnostics
            </a>
          ) : null}
        </div>
      </Panel>
    </div>
  );
}
