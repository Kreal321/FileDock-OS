import { DataTable } from '../../components/DataTable.tsx';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { api } from '../../api/client.ts';
import { dataView, displayTime } from '../../api/view-models.ts';
import { Notice } from '../../components/Notice.tsx';
import { SectionHeader } from '../../components/SectionHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { StateView } from '../../components/StateView.tsx';
import { Button } from '../../components/Button.tsx';
import { Select } from '../../components/Select.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
export type Alert = {
  id: string;
  severity: string;
  source: string;
  message: string;
  state: string;
  firstSeenAt: string;
  lastSeenAt: string;
};
export function CriticalAlertBanner() {
  const q = useQuery({
    queryKey: ['critical-alerts'],
    queryFn: () =>
      api.get<{ items: Alert[] }>('/alerts?state=Active&minSeverity=Critical&pageSize=100'),
    refetchInterval: 10000,
    retry: false,
  });
  return q.data?.items.length ? (
    <Notice tone="danger" role="alert">
      <strong>{q.data.items.length} critical condition(s) need attention.</strong>{' '}
      {q.data.items[0]?.message} <a href="#/settings/alerts">Review alerts</a>
    </Notice>
  ) : null;
}
export function AlertsSection({
  canMutate = false,
  connected = true,
}: {
  canMutate?: boolean;
  connected?: boolean;
}) {
  const cache = useQueryClient();
  const [state, setState] = useState('Active');
  const [severity, setSeverity] = useState('Info');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState('');
  const [pages, setPages] = useState<string[]>([]);
  const q = useQuery({
    queryKey: ['alerts', state, severity, pages.at(-1)],
    queryFn: () =>
      api.get<{ items: Alert[]; nextPageToken?: string }>(
        `/alerts?state=${state}&minSeverity=${severity}&pageSize=100&pageToken=${encodeURIComponent(pages.at(-1) ?? '')}`,
      ),
    refetchInterval: 10000,
    retry: false,
  });
  async function ack(id: string) {
    setBusy(id);
    setError('');
    try {
      await api.post(`/alerts/${id}/ack`);
      await cache.invalidateQueries({ predicate: (q) => String(q.queryKey[0]).includes('alert') });
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy('');
    }
  }
  return (
    <div className="stack">
      <SectionHeader
        title="Alerts"
        description="Conditions remain visible until acknowledged or resolved. Acknowledgement does not repair the cause."
      />
      {error ? <Notice tone="danger">{error}</Notice> : null}
      <Panel className="stack">
        <div className="actions">
          <Select
            label="State"
            value={state}
            onChange={(e) => {
              setState(e.target.value);
              setPages([]);
            }}
          >
            {['Active', 'Acknowledged', 'Resolved'].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </Select>
          <StateView
            {...dataView(q.data?.items, q.isPending, q.error, q.dataUpdatedAt, !connected)}
            onRetry={() => void q.refetch()}
            empty="No alerts in this state."
          />
          <Select
            label="Minimum severity"
            value={severity}
            onChange={(e) => {
              setSeverity(e.target.value);
              setPages([]);
            }}
          >
            {['Info', 'Warning', 'Critical'].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </Select>
        </div>
        {q.data?.items.length ? (
          <DataTable
            label="Alert conditions"
            headers={[
              'Severity',
              'Source / Condition',
              'State',
              { id: 'actions', label: 'Actions', align: 'actions' },
            ]}
          >
            {q.data.items.map((a) => (
              <tr key={a.id}>
                <td>
                  <StatusBadge state={a.severity} />
                </td>
                <td>
                  <strong> {a.source}</strong>
                  <p>{a.message}</p>
                  <p className="muted">
                    First seen {displayTime(a.firstSeenAt)} · Last seen {displayTime(a.lastSeenAt)}
                  </p>
                </td>
                <td>
                  <StatusBadge state={a.state} />
                </td>
                <td>
                  {a.state === 'Active' ? (
                    <Button
                      variant="secondary"
                      disabled={!canMutate || !connected || !!busy || !!q.error}
                      busy={busy === a.id}
                      onClick={() => void ack(a.id)}
                    >
                      Acknowledge
                    </Button>
                  ) : null}
                </td>
              </tr>
            ))}
          </DataTable>
        ) : null}
        <div className="actions">
          <Button
            disabled={!pages.length || q.isFetching}
            variant="secondary"
            onClick={() => setPages(pages.slice(0, -1))}
          >
            Previous
          </Button>
          <Button
            disabled={!q.data?.nextPageToken || q.isFetching}
            variant="secondary"
            onClick={() => setPages([...pages, q.data!.nextPageToken!])}
          >
            Next
          </Button>
        </div>
      </Panel>
    </div>
  );
}
