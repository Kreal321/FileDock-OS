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
import { Dialog } from '../../components/Dialog.tsx';
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
  const [selected, setSelected] = useState<Log | null>(null);
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
          className="form-grid log-filters"
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
          <div className="log-table">
            <DataTable
              label="Journal entries"
              headers={[
                { id: 'time', label: 'Time', width: '200px' },
                { id: 'source', label: 'Source', width: '240px' },
                { id: 'severity', label: 'Severity', width: '128px' },
                { id: 'message', label: 'Message' },
              ]}
            >
              {q.data.items.map((l, i) => (
                <tr key={i}>
                  <td className="log-time">{displayTime(l.at)}</td>
                  <td>
                    <span className="log-source" title={l.source}>
                      {l.source}
                    </span>
                  </td>
                  <td>
                    <StatusBadge state={l.severity} />
                  </td>
                  <td>
                    <button
                      type="button"
                      className="log-message-trigger"
                      aria-haspopup="dialog"
                      aria-label={`View log details from ${l.source} at ${displayTime(l.at)}, entry ${i + 1}`}
                      onClick={() => setSelected(l)}
                    >
                      <span className="log-message-preview">{l.message || '(Empty message)'}</span>
                      <span className="log-detail-hint" aria-hidden="true">
                        View details
                      </span>
                    </button>
                  </td>
                </tr>
              ))}
            </DataTable>
          </div>
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
            <a className="button secondary" href="/api/v1/diagnostics/bundle" download>
              Download diagnostics
            </a>
          ) : null}
        </div>
      </Panel>
      <Dialog
        open={!!selected}
        title="Log details"
        className="log-details-dialog"
        cancelLabel="Close"
        onClose={() => setSelected(null)}
      >
        {selected ? (
          <>
            <dl className="log-metadata">
              <dt>Time</dt>
              <dd>{displayTime(selected.at)}</dd>
              <dt>Source</dt>
              <dd>{selected.source}</dd>
              <dt>Severity</dt>
              <dd>
                <StatusBadge state={selected.severity} />
              </dd>
            </dl>
            <div className="stack">
              <h3>Message</h3>
              <pre className="log-message-full" tabIndex={0} aria-label="Full log message">
                {selected.message || '(Empty message)'}
              </pre>
            </div>
          </>
        ) : null}
      </Dialog>
    </div>
  );
}
