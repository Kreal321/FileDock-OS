import { PageHeader } from '../../components/PageHeader.tsx';
import { dataView } from '../../api/view-models.ts';
import { StateView } from '../../components/StateView.tsx';
import { TextAreaField } from '../../components/Field.tsx';
import { Button } from '../../components/Button.tsx';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { api } from '../../api/client.ts';
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
  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canMutate || busy) return;
    const form = new FormData(event.currentTarget);
    setBusy(true);
    setError('');
    setMessage('');
    try {
      const saved = await api.put<{ listenAddresses: string[] }>('/system/network', {
        listenAddresses: String(form.get('addresses'))
          .split(/[\s,]+/)
          .filter(Boolean),
        confirmDisconnect: form.get('confirmDisconnect') === 'on',
      });
      cache.setQueryData<Network>(['network'], (old) =>
        old ? { ...old, listenAddresses: saved.listenAddresses } : old,
      );
      setMessage(
        'Saved. The HTTP listeners will update shortly. Reconnect using one of the chosen addresses.',
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not update network settings.');
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
        empty="No records returned."
      />
      <p>
        The UI and API use HTTP on port {query.data?.port ?? 8021}. Choose local IP addresses, or
        use :: for all interfaces.
      </p>
      <p>
        If access is lost, run <code>filedockctl network reset</code> as root from the VM console.
      </p>
      {query.isPending ? <p role="status">Loading network settings…</p> : null}
      {query.data ? (
        <form className="panel stack" onSubmit={save}>
          <TextAreaField
            label="Listen addresses (one per line)"
            name="addresses"
            required
            defaultValue={query.data.listenAddresses.join('\n')}
          />
          <ul>
            {query.data.availableAddresses.map((address) => (
              <li key={`${address.interfaceName}-${address.address}`}>
                {address.interfaceName}: {address.address}
              </li>
            ))}
          </ul>
          <label className="inline-label">
            <input name="confirmDisconnect" type="checkbox" />
            Allow this change to disconnect my current connection
          </label>
          <Button variant="primary" disabled={!canMutate} busy={busy} type="submit">
            Save network settings
          </Button>
        </form>
      ) : null}
      {error || query.error ? (
        <p className="error" role="alert">
          {error || query.error?.message}
        </p>
      ) : null}
      {message ? <p role="status">{message}</p> : null}
    </section>
  );
}
