import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api } from '../../api/client.ts';
import { dataView, displayTime } from '../../api/view-models.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { StateView } from '../../components/StateView.tsx';
import { Button } from '../../components/Button.tsx';
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
export function ActivityPage({ connected = true }: { connected?: boolean }) {
  const [pages, setPages] = useState<string[]>([]);
  const q = useQuery({
    queryKey: ['activity', pages.at(-1)],
    queryFn: () =>
      api.get<{ items: Event[]; nextPageToken?: string }>(
        `/activity?pageSize=50&pageToken=${encodeURIComponent(pages.at(-1) ?? '')}`,
      ),
    retry: false,
  });
  return (
    <div className="stack">
      <PageHeader
        title="Activity"
        description="Jobs, configuration changes, and protection or health updates. Logs contain technical diagnostics; Audit records administrator actions. File access is not collected."
      />
      <Panel className="stack">
        <StateView
          {...dataView(q.data?.items, q.isPending, q.error, q.dataUpdatedAt, !connected)}
          onRetry={() => void q.refetch()}
          empty="No appliance activity yet."
        />
        <div className="activity-list">
          {groupActivity(q.data?.items ?? []).map(({ event: e, count, oldestAt }) => (
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
        <div className="actions">
          <Button
            disabled={!pages.length}
            variant="secondary"
            onClick={() => setPages(pages.slice(0, -1))}
          >
            Previous
          </Button>
          <Button
            disabled={!q.data?.nextPageToken}
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
