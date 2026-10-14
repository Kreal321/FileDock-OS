import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { api } from '../../api/client.ts';
import { dataView } from '../../api/view-models.ts';
import { Notice } from '../../components/Notice.tsx';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { StateView } from '../../components/StateView.tsx';
import { Button } from '../../components/Button.tsx';
import { Field, TextAreaField } from '../../components/Field.tsx';
import { Select } from '../../components/Select.tsx';
import { Dialog } from '../../components/Dialog.tsx';
type Channel = {
  id: string;
  name: string;
  subsystemId: string;
  minSeverity: string;
  config: Record<string, unknown>;
  secretRefs: string[];
};
export function NotificationChannelsPage({ canMutate = false }: { canMutate?: boolean }) {
  const cache = useQueryClient();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState<Channel | null | undefined>();
  const [removing, setRemoving] = useState<Channel | null>(null);
  const [notice, setNotice] = useState('');
  const q = useQuery({
    queryKey: ['notification-channels'],
    queryFn: () => api.get<{ items: Channel[] }>('/notification-channels'),
    retry: false,
  });
  const providers = useQuery({
    queryKey: ['subsystems'],
    queryFn: () =>
      api.get<{ subsystems: { subsystemId: string; name: string; capabilities: string[] }[] }>(
        '/subsystems',
      ),
    retry: false,
  });
  async function action(work: () => Promise<unknown>) {
    setBusy(true);
    setError('');
    try {
      await work();
      await cache.invalidateQueries({ queryKey: ['notification-channels'] });
      await cache.invalidateQueries({ queryKey: ['system-status'] });
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="stack">
      <PageHeader
        title="Notification channels"
        description="Alerts stay in the UI even when external delivery fails. Channels are provided by installed extensions."
      />
      {error ? <Notice tone="danger">{error}</Notice> : null}
      {notice ? <Notice tone="success">{notice}</Notice> : null}
      <Panel className="stack">
        <StateView
          {...dataView(q.data?.items, q.isPending, q.error, q.dataUpdatedAt)}
          onRetry={() => void q.refetch()}
          empty="No notification channel is configured. Alerts are visible only in the web UI."
        />
        {q.data?.items.map((c) => (
          <article className="resource-row" key={c.id}>
            <div>
              <strong>{c.name}</strong>
              <p>
                {c.subsystemId} · {c.minSeverity} and above
              </p>
            </div>
            <div className="actions">
              <Button
                variant="secondary"
                disabled={!canMutate || busy || !!q.error}
                onClick={() =>
                  void action(async () => {
                    await api.post(`/notification-channels/${c.id}/test`);
                    setNotice(`Test delivered to ${c.name}.`);
                  })
                }
              >
                Send test
              </Button>
              <Button
                variant="secondary"
                disabled={!canMutate || busy || !!q.error}
                onClick={() => setEditing(c)}
              >
                Edit
              </Button>
              <Button
                variant="danger"
                disabled={!canMutate || busy || !!q.error}
                onClick={() => setRemoving(c)}
              >
                Remove
              </Button>
            </div>
          </article>
        ))}
        <Button
          disabled={
            !canMutate ||
            busy ||
            !!q.error ||
            !providers.data?.subsystems.some((p) => p.capabilities.includes('notify.channel'))
          }
          onClick={() => setEditing(null)}
        >
          Add channel
        </Button>
        {!providers.data?.subsystems.some((p) => p.capabilities.includes('notify.channel')) ? (
          <p className="muted">
            Install an extension that provides notify.channel to add a channel.
          </p>
        ) : null}
      </Panel>
      <Dialog
        open={editing !== undefined}
        title={editing ? 'Edit channel' : 'Add channel'}
        onClose={() => setEditing(undefined)}
      >
        {editing !== undefined ? (
          <form
            className="stack"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void action(async () => {
                const body = {
                  name: String(f.get('name')),
                  subsystemId: String(f.get('provider')),
                  minSeverity: String(f.get('severity')),
                  config: JSON.parse(String(f.get('config')) || '{}'),
                  secretRefs: String(f.get('secretRefs'))
                    .split(',')
                    .map((s) => s.trim())
                    .filter(Boolean),
                };
                if (editing) await api.patch(`/notification-channels/${editing.id}`, body);
                else await api.post('/notification-channels', body);
                setEditing(undefined);
              });
            }}
          >
            <Field label="Name" name="name" required maxLength={128} defaultValue={editing?.name} />
            <Select label="Extension" name="provider" defaultValue={editing?.subsystemId}>
              {providers.data?.subsystems
                .filter((p) => p.capabilities.includes('notify.channel'))
                .map((p) => (
                  <option key={p.subsystemId} value={p.subsystemId}>
                    {p.name}
                  </option>
                ))}
            </Select>
            <Select
              label="Minimum severity"
              name="severity"
              defaultValue={editing?.minSeverity ?? 'Warning'}
            >
              {['Info', 'Warning', 'Critical'].map((s) => (
                <option key={s}>{s}</option>
              ))}
            </Select>
            <TextAreaField
              label="Channel configuration (JSON)"
              name="config"
              defaultValue={JSON.stringify(editing?.config ?? {}, null, 2)}
              hint="Use the extension’s configuration guide. Credentials must use secret references."
            />
            <Field
              label="Granted secret references (comma separated)"
              name="secretRefs"
              defaultValue={editing?.secretRefs.join(',')}
            />
            <Button disabled={!canMutate || busy} type="submit">
              Save channel
            </Button>
          </form>
        ) : null}
      </Dialog>
      <Dialog
        open={!!removing}
        title="Remove notification channel"
        onClose={() => setRemoving(null)}
      >
        <p>Alerts will no longer be delivered to {removing?.name}. Existing UI alerts are kept.</p>
        <Button
          variant="danger"
          disabled={!canMutate || busy}
          onClick={() =>
            void action(async () => {
              await api.delete(`/notification-channels/${removing!.id}`);
              setRemoving(null);
            })
          }
        >
          Remove channel
        </Button>
      </Dialog>
    </div>
  );
}
