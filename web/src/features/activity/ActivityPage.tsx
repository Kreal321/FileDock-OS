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
        description="Appliance events, newest first. File access activity is not collected here."
      />
      <Panel className="stack">
        <StateView
          {...dataView(q.data?.items, q.isPending, q.error, q.dataUpdatedAt, !connected)}
          onRetry={() => void q.refetch()}
          empty="No appliance activity yet."
        />
        {q.data?.items.map((e) => (
          <article key={e.seq} className="resource-row">
            <div>
              <strong>{e.type}</strong>
              <p className="muted">
                {displayTime(e.at)} · {e.subject?.id ?? 'Appliance'}
              </p>
              {typeof e.payload?.detail === 'string' ? <p>{e.payload.detail}</p> : null}
            </div>
          </article>
        ))}
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
