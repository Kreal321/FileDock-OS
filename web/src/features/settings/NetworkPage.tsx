import { useId, useState, type FormEvent } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api } from '../../api/client.ts';
import { dataView } from '../../api/view-models.ts';
import { useUnsavedChanges } from '../../navigation/useUnsavedChanges.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { SettingsRow } from '../../components/SettingsRow.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import { StateView } from '../../components/StateView.tsx';
import { TextAreaField } from '../../components/Field.tsx';
import { Button } from '../../components/Button.tsx';
type Network = {
  listenAddresses: string[];
  port: number;
  availableAddresses: { interfaceName: string; address: string }[];
};
export function NetworkPage({
  canMutate = true,
  connected = true,
}: {
  canMutate?: boolean;
  connected?: boolean;
}) {
  const cache = useQueryClient();
  const query = useQuery({
    queryKey: ['network'],
    queryFn: () => api.get<Network>('/system/network'),
  });
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);
  const [draft, setDraft] = useState<string | null>(null);
  const [ack, setAck] = useState(false);
  const dirty = draft !== null && (draft !== query.data?.listenAddresses.join('\n') || ack);
  const formId = useId();
  useUnsavedChanges(dirty);
  function close() {
    setDraft(null);
    setAck(false);
    setError('');
  }
  async function save(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!canMutate || busy) return;
    setBusy(true);
    setError('');
    setMessage('');
    try {
      const saved = await api.put<{ listenAddresses: string[] }>('/system/network', {
        listenAddresses: (draft ?? '').split(/[\s,]+/).filter(Boolean),
        confirmDisconnect: ack,
      });
      cache.setQueryData<Network>(['network'], (old) =>
        old ? { ...old, listenAddresses: saved.listenAddresses } : old,
      );
      close();
      setMessage(
        'Saved. The HTTP listeners will update shortly. Reconnect using one of the chosen addresses.',
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not update network settings.');
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="stack">
      <PageHeader title="Network settings" description="Local access to appliance management." />
      <StateView
        {...dataView(query.data, query.isPending, query.error, query.dataUpdatedAt, !connected)}
        onRetry={() => void query.refetch()}
        empty="Network settings unavailable."
      />
      {query.data ? (
        <Panel>
          <SettingsRow
            title="Management listeners"
            description={`HTTP on port ${query.data.port}`}
            action={
              <Button
                variant="secondary"
                size="compact"
                disabled={!canMutate}
                onClick={() => {
                  setDraft(query.data!.listenAddresses.join('\n'));
                  setAck(false);
                  setError('');
                }}
              >
                Configure network
              </Button>
            }
          >
            <p>{query.data.listenAddresses.join(', ')}</p>
          </SettingsRow>
          <SettingsRow
            title="Console recovery"
            description={
              <>
                If access is lost, run <code>filedockctl network reset</code> as root from the VM
                console.
              </>
            }
          />
        </Panel>
      ) : null}
      {message ? <p role="status">{message}</p> : null}
      <Dialog
        open={draft !== null}
        title="Configure network"
        description={`The UI and API use HTTP on port ${query.data?.port ?? 8021}. Choose local IP addresses, or use :: for all interfaces.`}
        onClose={close}
        dirty={dirty}
        busy={busy}
        footer={
          <Button form={formId} type="submit" disabled={!canMutate} busy={busy}>
            Save network settings
          </Button>
        }
      >
        {error ? (
          <p className="error" role="alert">
            {error}
          </p>
        ) : null}
        <form id={formId} className="stack" onSubmit={save}>
          <TextAreaField
            label="Listen addresses (one per line)"
            name="addresses"
            required
            value={draft ?? ''}
            onChange={(e) => setDraft(e.target.value)}
          />
          <div>
            <h3>Available addresses</h3>
            <ul>
              {query.data?.availableAddresses.map((a) => (
                <li key={`${a.interfaceName}-${a.address}`}>
                  {a.interfaceName}: {a.address}
                </li>
              ))}
            </ul>
          </div>
          <label className="inline-label">
            <input
              name="confirmDisconnect"
              type="checkbox"
              checked={ack}
              onChange={(e) => setAck(e.target.checked)}
            />
            Allow this change to disconnect my current connection
          </label>
          <p className="caption muted">
            Console recovery: <code>filedockctl network reset</code>
          </p>
        </form>
      </Dialog>
    </section>
  );
}
