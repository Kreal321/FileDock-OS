import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api } from '../../api/client.ts';
import { dataView, displayTime } from '../../api/view-models.ts';
import { Button } from '../../components/Button.tsx';
import { StateView } from '../../components/StateView.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import {
  previousPageCursor,
  type RecordType,
  type ViewFilters,
} from '../../navigation/viewContext.ts';
type Event = {
  seq: number;
  at: string;
  type: string;
  subject?: { kind: string; id: string };
  payload?: Record<string, unknown>;
};
type ActivityGroup = { event: Event; count: number; oldestAt: string };
function activityPayload(value: unknown): string {
  return JSON.stringify(value ?? {}, (_key, item: unknown) =>
    item && typeof item === 'object' && !Array.isArray(item)
      ? Object.fromEntries(Object.entries(item).sort(([a], [b]) => a.localeCompare(b)))
      : item,
  );
}
function groupActivity(events: Event[]): ActivityGroup[] {
  const groups: ActivityGroup[] = [];
  for (const event of events) {
    const previous = groups.at(-1);
    if (
      previous &&
      previous.event.type === event.type &&
      previous.event.subject?.id === event.subject?.id &&
      previous.event.subject?.kind === event.subject?.kind &&
      activityPayload(previous.event.payload) === activityPayload(event.payload)
    ) {
      previous.count++;
      previous.oldestAt = event.at;
    } else groups.push({ event, count: 1, oldestAt: event.at });
  }
  return groups;
}
const names: Record<string, string> = {
  'state.protection.changed': 'Recovery protection updated',
  'operation.started': 'Job started',
  'operation.completed': 'Job completed',
  'operation.failed': 'Job failed',
  'operation.rolled_back': 'Job rolled back',
  'alert.changed': 'Alert updated',
  'folder.configuration_changed': 'Folder configuration changed',
};
function activityName(type: string) {
  return names[type] ?? type.replace(/[._]/g, ' ').replace(/^./, (s) => s.toUpperCase());
}

type Log = { at: string; source: string; severity: string; message: string };
type Audit = {
  id: string;
  at: string;
  actorName: string;
  action: string;
  target: string;
  result: string;
};
type Records = { items: (Log & Audit & Event)[]; nextPageToken?: string };
export function RecordView({
  type,
  filters,
  connected = true,
  canMutate = false,
  page,
}: {
  type: RecordType;
  filters: ViewFilters;
  connected?: boolean;
  canMutate?: boolean;
  page: (cursor: string, direction: 'next' | 'previous') => void;
}) {
  const previous = previousPageCursor(type, filters);
  const [selected, setSelected] = useState<Log | null>(null);
  const size = filters.pageSize ?? { logs: '100', activity: '50', audit: '25' }[type];
  const params = new URLSearchParams({ ...filters, pageSize: size });
  const q = useQuery({
    queryKey: [type, filters],
    queryFn: ({ signal }) => api.get<Records>(`/${type}?${params}`, signal),
    retry: false,
  });
  const items = q.data?.items ?? [];
  return (
    <div className="stack event-records">
      <StateView
        {...dataView(q.data?.items, q.isPending, q.error, q.dataUpdatedAt, !connected)}
        onRetry={() => void q.refetch()}
        empty={
          type === 'activity'
            ? 'No appliance activity yet.'
            : type === 'logs'
              ? 'No journal entries match these filters.'
              : 'No administrative activity matches these filters.'
        }
      />
      {type === 'activity' ? (
        <div className="activity-list">
          {groupActivity(items).map(({ event: e, count, oldestAt }) => (
            <article key={e.seq} className="activity-row">
              <strong>{activityName(e.type)}</strong>
              <p className="caption muted">
                {displayTime(e.at)} · {e.subject?.id ?? 'Appliance'}
              </p>
              {count > 1 ? (
                <p className="caption muted">
                  {count} identical updates · {displayTime(oldestAt)} – {displayTime(e.at)}
                </p>
              ) : null}
              {typeof e.payload?.detail === 'string' ? <p>{e.payload.detail}</p> : null}
            </article>
          ))}
        </div>
      ) : null}
      {type === 'logs' && items.length ? (
        <div className="log-table">
          <DataTable
            label="Journal entries"
            headers={[
              { id: 'time', label: 'Time', width: '180px' },
              { id: 'source', label: 'Source', width: '200px' },
              { id: 'severity', label: 'Severity', width: '120px' },
              { id: 'message', label: 'Message' },
            ]}
          >
            {items.map((item, i) => (
              <tr key={i}>
                <td className="log-time">{displayTime(item.at)}</td>
                <td>
                  <span className="log-source" title={item.source}>
                    {item.source}
                  </span>
                </td>
                <td>
                  <StatusBadge state={item.severity} />
                </td>
                <td>
                  <button
                    type="button"
                    className="log-message-trigger"
                    aria-haspopup="dialog"
                    aria-label={`View log details from ${item.source} at ${displayTime(item.at)}, entry ${i + 1}`}
                    onClick={() => setSelected(item)}
                  >
                    <span className="log-message-preview">{item.message || '(Empty message)'}</span>
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
      {type === 'audit' && items.length ? (
        <DataTable
          label="Administrative activity"
          headers={['Time', 'Actor', 'Action', 'Target', 'Result']}
        >
          {items.map((item) => (
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
          disabled={previous === undefined || q.isFetching}
          onClick={() => page(previous!, 'previous')}
        >
          Previous
        </Button>
        <Button
          variant="secondary"
          disabled={!q.data?.nextPageToken || q.isFetching}
          onClick={() => page(q.data!.nextPageToken!, 'next')}
        >
          Next
        </Button>
        {type === 'logs' && canMutate && connected ? (
          <a className="button secondary" href="/api/v1/diagnostics/bundle" download>
            Download diagnostics
          </a>
        ) : null}
      </div>
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
            <h3>Message</h3>
            <pre className="log-message-full" tabIndex={0} aria-label="Full log message">
              {selected.message || '(Empty message)'}
            </pre>
          </>
        ) : null}
      </Dialog>
    </div>
  );
}
