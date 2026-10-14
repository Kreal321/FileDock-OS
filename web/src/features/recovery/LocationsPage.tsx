import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api } from '../../api/client.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { Notice } from '../../components/Notice.tsx';
import { Button } from '../../components/Button.tsx';
type Location = {
  connectionId: string;
  bucket: string;
  prefix: string;
  purpose: string;
  ownerApplianceId: string;
  generation: string;
  lastWriteAt?: string;
  role: string;
  servedByConnector: string;
  servedByThirdParty?: boolean;
  claimSafety: string;
};
export function LocationsPage({ canMutate = true }: { canMutate?: boolean }) {
  const q = useQuery({
    queryKey: ['owned-locations'],
    queryFn: () => api.get<{ locations?: Location[] }>('/locations'),
    refetchInterval: 10000,
  });
  const [error, setError] = useState('');
  const [busy, setBusy] = useState('');
  async function takeover(l: Location) {
    const recent =
      !l.lastWriteAt || q.dataUpdatedAt - new Date(l.lastWriteAt).getTime() < 15 * 60 * 1000;
    if (
      !window.confirm(
        `${recent ? 'This location has a recent writer. Stop the previous appliance first. ' : ''}Take over ${l.bucket}/${l.prefix} at generation ${l.generation}?`,
      )
    )
      return;
    setBusy(l.connectionId);
    setError('');
    try {
      await api.post('/locations/takeover', {
        connectionId: l.connectionId,
        bucket: l.bucket,
        prefix: l.prefix,
        observedGeneration: l.generation,
        acknowledgedRecentWriter: recent,
      });
      await q.refetch();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Takeover failed.');
    } finally {
      setBusy('');
    }
  }
  return (
    <div className="stack">
      <PageHeader
        title="Owned locations"
        description="One writer per location. Recovery readers do not write, probe with writes, or remove objects."
      />
      {error && (
        <Notice tone="danger" role="alert">
          {error}
        </Notice>
      )}
      {q.isError && (
        <Notice role="alert">
          Locations unavailable. <Button onClick={() => void q.refetch()}>Retry</Button>
        </Notice>
      )}
      {q.isPending && <Panel>Loading locations…</Panel>}
      {q.data && !q.data.locations?.length && <Panel>No owned locations registered.</Panel>}
      {q.data?.locations?.map((l) => (
        <Panel className="stack" key={`${l.connectionId}:${l.bucket}:${l.prefix}`}>
          <h2>
            {l.bucket}/{l.prefix}
          </h2>
          <p>
            {l.purpose} · {l.role} · Generation {l.generation}
          </p>
          <p>
            Owner {l.ownerApplianceId} · Last write {l.lastWriteAt ?? 'unknown'}
          </p>
          <p>
            Connector {l.servedByConnector}
            {l.servedByThirdParty ? ' (third party)' : ''} · Claim safety {l.claimSafety}
          </p>
          {l.claimSafety === 'Basic' && (
            <Notice>
              Basic claim safety uses write-then-verify. Avoid simultaneous installations.
            </Notice>
          )}
          {l.role !== 'Writer' && (
            <Button
              disabled={!canMutate}
              busy={busy === l.connectionId}
              onClick={() => void takeover(l)}
            >
              Take over location
            </Button>
          )}
        </Panel>
      ))}
    </div>
  );
}
