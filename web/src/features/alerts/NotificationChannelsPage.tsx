import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useId, useRef, useState } from 'react';
import { api } from '../../api/client.ts';
import { dataView } from '../../api/view-models.ts';
import { useUnsavedChanges } from '../../navigation/useUnsavedChanges.ts';
import { Notice } from '../../components/Notice.tsx';
import { SectionHeader } from '../../components/SectionHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { StateView } from '../../components/StateView.tsx';
import { Button } from '../../components/Button.tsx';
import { Field, TextAreaField } from '../../components/Field.tsx';
import { Select } from '../../components/Select.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { RowActions } from '../../components/RowActions.tsx';
type Channel = {
  id: string;
  name: string;
  subsystemId: string;
  minSeverity: string;
  config: Record<string, unknown>;
  secretRefs: string[];
};
type Action = { kind: 'edit'; channel?: Channel } | { kind: 'remove' | 'test'; channel: Channel };
export function NotificationChannelsSection({
  canMutate = false,
  connected = true,
}: {
  canMutate?: boolean;
  connected?: boolean;
}) {
  const cache = useQueryClient(),
    formId = useId();
  const [error, setError] = useState(''),
    [notice, setNotice] = useState('');
  const [busy, setBusy] = useState(false),
    [dirty, setDirty] = useState(false);
  const [editing, setEditing] = useState<Action | null>(null);
  const epoch = useRef(0);
  useUnsavedChanges(dirty);
  useEffect(() => {
    const clear = () => {
      epoch.current++;
      setEditing(null);
      setDirty(false);
      setError('');
      setNotice('');
    };
    window.addEventListener('filedock:session-expired', clear);
    return () => {
      clear();
      window.removeEventListener('filedock:session-expired', clear);
    };
  }, []);
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
  const available =
    providers.data?.subsystems.filter((p) => p.capabilities.includes('notify.channel')) ?? [];
  const enabled = canMutate && connected && !busy && !q.error;
  function close() {
    epoch.current++;
    setEditing(null);
    setDirty(false);
    setError('');
  }
  function open(action: Action) {
    epoch.current++;
    setEditing(action);
    setDirty(false);
    setError('');
    setNotice('');
  }
  async function action(work: () => Promise<unknown>, success?: string) {
    if (!enabled) return;
    const current = epoch.current;
    setBusy(true);
    setError('');
    try {
      await work();
      if (current !== epoch.current) return;
      setEditing(null);
      setDirty(false);
      if (success) setNotice(success);
      await cache.invalidateQueries({ queryKey: ['notification-channels'] });
      await cache.invalidateQueries({ queryKey: ['system-status'] });
    } catch (e) {
      if (current === epoch.current) setError((e as Error).message);
    } finally {
      if (current === epoch.current) setBusy(false);
    }
  }
  return (
    <section className="stack">
      <SectionHeader
        title="Delivery channels"
        description="Channel delivery is separate from alert acknowledgement or condition resolution."
        action={
          <Button disabled={!enabled || !available.length} onClick={() => open({ kind: 'edit' })}>
            Add channel
          </Button>
        }
      />
      {notice ? (
        <Notice tone="success" role="status">
          {notice}
        </Notice>
      ) : null}
      <Panel className="stack">
        <StateView
          {...dataView(q.data?.items, q.isPending, q.error, q.dataUpdatedAt, !connected)}
          onRetry={() => void q.refetch()}
          empty="No notification channel is configured. Alerts are visible only in the web UI."
        />
        {providers.error ? (
          <Notice tone="warning">Channel providers unavailable. {providers.error.message}</Notice>
        ) : null}
        {!available.length ? (
          <p className="muted">
            Install an extension that provides notify.channel to add a channel.
          </p>
        ) : null}
        {q.data?.items.length ? (
          <DataTable
            label="Delivery channels"
            headers={[
              'Name',
              'Extension',
              'Minimum severity',
              { id: 'actions', label: 'Actions', align: 'actions' },
            ]}
          >
            {q.data.items.map((c) => (
              <tr key={c.id}>
                <td>{c.name}</td>
                <td>{c.subsystemId}</td>
                <td>{c.minSeverity} and above</td>
                <td>
                  <RowActions
                    label={`Actions for ${c.name}`}
                    actions={[
                      {
                        label: `Send test to ${c.name}`,
                        disabled: !enabled,
                        onClick: () => open({ kind: 'test', channel: c }),
                      },
                      {
                        label: `Edit ${c.name}`,
                        disabled:
                          !enabled || !available.some((p) => p.subsystemId === c.subsystemId),
                        onClick: () => open({ kind: 'edit', channel: c }),
                      },
                      {
                        label: `Remove ${c.name}`,
                        disabled: !enabled,
                        danger: true,
                        onClick: () => open({ kind: 'remove', channel: c }),
                      },
                    ]}
                  />
                </td>
              </tr>
            ))}
          </DataTable>
        ) : null}
      </Panel>
      <Dialog
        open={!!editing}
        title={
          editing?.kind === 'edit'
            ? editing.channel
              ? 'Edit channel'
              : 'Add channel'
            : editing?.kind === 'test'
              ? 'Send test notification'
              : 'Remove notification channel'
        }
        onClose={close}
        dirty={dirty}
        busy={busy}
        footer={
          editing?.kind === 'edit' ? (
            <Button form={formId} type="submit" disabled={!enabled} busy={busy}>
              Save channel
            </Button>
          ) : (
            <Button
              variant={editing?.kind === 'remove' ? 'danger' : 'primary'}
              disabled={!enabled}
              busy={busy}
              onClick={() => {
                if (!editing) return;
                const c = editing.channel;
                void action(
                  () =>
                    editing.kind === 'test'
                      ? api.post(`/notification-channels/${encodeURIComponent(c.id)}/test`)
                      : api.delete(`/notification-channels/${encodeURIComponent(c.id)}`),
                  editing.kind === 'test' ? `Test delivered to ${c.name}.` : undefined,
                );
              }}
            >
              {editing?.kind === 'test' ? 'Send test' : 'Remove channel'}
            </Button>
          )
        }
      >
        {error ? <p role="alert">{error}</p> : null}
        {editing?.kind === 'edit' ? (
          <form
            id={formId}
            className="stack"
            onChange={() => setDirty(true)}
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void action(async () => {
                const body = {
                  name: String(f.get('name')),
                  subsystemId: String(f.get('provider')),
                  minSeverity: String(f.get('severity')),
                  config: JSON.parse(String(f.get('config')) || '{}') as Record<string, unknown>,
                  secretRefs: String(f.get('secretRefs'))
                    .split(',')
                    .map((s) => s.trim())
                    .filter(Boolean),
                };
                if (editing.channel)
                  await api.patch(
                    `/notification-channels/${encodeURIComponent(editing.channel.id)}`,
                    body,
                  );
                else await api.post('/notification-channels', body);
              });
            }}
          >
            <Field
              label="Name"
              name="name"
              required
              maxLength={128}
              defaultValue={editing.channel?.name}
            />
            <Select label="Extension" name="provider" defaultValue={editing.channel?.subsystemId}>
              {available.map((p) => (
                <option key={p.subsystemId} value={p.subsystemId}>
                  {p.name}
                </option>
              ))}
            </Select>
            <Select
              label="Minimum severity"
              name="severity"
              defaultValue={editing.channel?.minSeverity ?? 'Warning'}
            >
              {['Info', 'Warning', 'Critical'].map((s) => (
                <option key={s}>{s}</option>
              ))}
            </Select>
            <TextAreaField
              label="Channel configuration (JSON)"
              name="config"
              defaultValue={JSON.stringify(editing.channel?.config ?? {}, null, 2)}
              hint="Use secret references for credentials."
            />
            <Field
              label="Granted secret references (comma separated)"
              name="secretRefs"
              defaultValue={editing.channel?.secretRefs.join(',')}
            />
          </form>
        ) : (
          <p>
            {editing?.kind === 'test'
              ? `Send a real test to ${editing.channel.name}. Success is shown only after delivery completes.`
              : `Remove ${editing?.channel.name}? Existing UI alerts are kept.`}
          </p>
        )}
      </Dialog>
    </section>
  );
}
