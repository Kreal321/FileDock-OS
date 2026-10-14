import { useQuery } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { api } from '../../api/client.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { Notice } from '../../components/Notice.tsx';
import { Button } from '../../components/Button.tsx';
import { Field } from '../../components/Field.tsx';
import { RecoveryKeyStep } from '../setup/RecoveryKeyStep.tsx';
export type Protection = {
  remoteDestinationRegistered?: boolean;
  reinstallCovered?: boolean;
  machineLossCovered?: boolean;
  warning?: boolean;
  latestChangeAt?: string;
  copies?: {
    kind: string;
    poolId?: string;
    description: string;
    lastSnapshotAt?: string;
    stale?: boolean;
    lastError?: string;
  }[];
};
export function StateProtectionPage({ canMutate = true }: { canMutate?: boolean }) {
  const q = useQuery({
    queryKey: ['state-protection'],
    queryFn: () => api.get<Protection>('/state-protection'),
    refetchInterval: 10000,
  });
  const [key, setKey] = useState<{ keyId: string; recoveryKey: string } | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [operation, setOperation] = useState('');
  const [connectionId, setConnectionId] = useState('');
  async function run(action: () => Promise<void>) {
    if (busy || !canMutate) return;
    setBusy(true);
    setError('');
    try {
      await action();
      await q.refetch();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Request failed.');
    } finally {
      setBusy(false);
    }
  }
  async function destination(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = new FormData(e.currentTarget);
    await run(async () => {
      const disk = String(form.get('disk') ?? '').trim();
      const connection = String(form.get('connection') ?? '').trim();
      if (Boolean(disk) === Boolean(connection))
        throw new Error('Choose either a mounted disk or an object-storage connection.');
      const op = await api.put<{ id: string }>('/state-protection/destination', {
        removableDiskPath: disk || undefined,
        objectStorage: connection
          ? {
              connectionId: connection,
              bucket: String(form.get('bucket') ?? ''),
              prefix: String(form.get('prefix') ?? ''),
            }
          : undefined,
        acknowledgedBasicClaimSafety: form.get('basic') === 'on',
        acknowledgedThirdPartyConnector: form.get('third') === 'on',
      });
      setOperation(op.id);
    });
  }
  async function download() {
    await run(async () => {
      const response = await fetch('/api/v1/state-protection/backup-file', {
        credentials: 'same-origin',
      });
      if (!response.ok)
        throw new Error('Could not export configuration. Retry after a successful snapshot.');
      const url = URL.createObjectURL(await response.blob());
      const link = document.createElement('a');
      link.href = url;
      link.download = 'filedock.fdbackup';
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    });
  }
  return (
    <div className="stack">
      <PageHeader
        title="Appliance state protection"
        description="Encrypted configuration and credentials. File data is protected by its storage feature."
      />
      {error && (
        <Notice tone="danger" role="alert">
          {error}
        </Notice>
      )}
      {q.isError && (
        <Notice role="alert">
          Protection status could not be loaded.{' '}
          <Button onClick={() => void q.refetch()}>Retry</Button>
        </Notice>
      )}
      {q.isPending ? (
        <Panel>Loading protection status…</Panel>
      ) : (
        q.data && (
          <>
            <Notice tone={q.error || q.data.warning ? 'warning' : 'success'}>
              {q.error && 'Cached status — current coverage could not be checked. '}
              Reinstall:{' '}
              {q.data.reinstallCovered
                ? 'covered by a data-pool copy'
                : 'not covered by a data-pool copy'}
              . Machine loss:{' '}
              {q.data.machineLossCovered
                ? 'covered by a remote copy'
                : 'not covered by a remote copy'}
              . Changes after the last durable snapshot can be lost.
            </Notice>
            <Panel className="stack">
              <h2>Recovery copies</h2>
              {(q.data.copies ?? []).map((copy, i) => (
                <div key={i}>
                  <strong>{copy.description}</strong>
                  <p>
                    {copy.kind} ·{' '}
                    {copy.lastSnapshotAt
                      ? `Last snapshot: ${new Date(copy.lastSnapshotAt).toLocaleString()}`
                      : 'No durable snapshot yet'}{' '}
                    · {q.error ? 'Cached' : copy.stale ? 'Stale' : 'Current'}
                  </p>
                  {copy.lastError && <p className="error">{copy.lastError}</p>}
                </div>
              ))}
              {!q.data.copies?.length && <p>No copies reported yet.</p>}
            </Panel>
          </>
        )
      )}
      <Panel className="stack">
        <h2>Save a recovery point</h2>
        <div className="actions">
          <Button
            disabled={!canMutate}
            busy={busy}
            onClick={() =>
              void run(async () => {
                const op = await api.post<{ id: string }>('/state-protection/snapshot');
                setOperation(op.id);
              })
            }
          >
            Snapshot now
          </Button>
          <Button disabled={!canMutate} busy={busy} onClick={() => void download()}>
            Download configuration backup
          </Button>
        </div>
        <p>The .fdbackup needs your recovery key to restore. Store both off this appliance.</p>
        {operation && <a href={`#/settings/operations/${operation}`}>View operation progress</a>}
      </Panel>
      <form
        className="panel stack"
        onSubmit={(e) => {
          e.preventDefault();
          const element = e.currentTarget;
          const form = new FormData(element);
          void run(async () => {
            const result = await api.post<{ connectionId: string }>(
              '/state-protection/connections',
              {
                name: String(form.get('name') ?? ''),
                endpoint: String(form.get('endpoint') ?? ''),
                region: String(form.get('region') ?? ''),
                bucket: String(form.get('bucket') ?? ''),
                accessKey: String(form.get('access') ?? ''),
                secretKey: String(form.get('secret') ?? ''),
              },
            );
            setConnectionId(result.connectionId);
            element.reset();
          });
        }}
      >
        <h2>Create a backup connection</h2>
        <Field label="Connection name" name="name" required />
        <Field label="S3-compatible HTTPS endpoint" name="endpoint" type="url" required />
        <Field label="Region" name="region" defaultValue="us-east-1" required />
        <Field label="Bucket" name="bucket" required />
        <Field label="Access key" name="access" required autoComplete="off" />
        <Field label="Secret key" name="secret" type="password" required autoComplete="off" />
        <Button variant="primary" type="submit" busy={busy} disabled={!canMutate}>
          Create and test connection
        </Button>
        {connectionId && (
          <p>
            Connection created: {connectionId}. Use it below to register the dedicated backup
            prefix.
          </p>
        )}
      </form>
      <form className="panel stack" onSubmit={destination}>
        <h2>Remote destination</h2>
        <Field label="Mounted removable disk path" name="disk" placeholder="/mnt/recovery" />
        <p>Or select an existing object-storage connection:</p>
        <Field
          label="Connection ID"
          name="connection"
          value={connectionId}
          onChange={(e) => setConnectionId(e.target.value)}
        />
        <Field label="Bucket" name="bucket" />
        <Field label="Dedicated empty prefix" name="prefix" />
        <label>
          <input name="basic" type="checkbox" /> I accept basic write-then-verify claim safety
          without conditional writes.
        </label>
        <label>
          <input name="third" type="checkbox" /> I trust the third-party connector serving this
          destination.
        </label>
        <Button variant="primary" type="submit" busy={busy} disabled={!canMutate}>
          Register destination
        </Button>
      </form>
      <Panel className="stack">
        <h2>Recovery key</h2>
        <p>
          Regeneration makes a new key necessary for future snapshots. Keep the old key for older
          copies that have not been updated.
        </p>
        <Button
          variant="danger"
          disabled={!canMutate || key !== null}
          busy={busy}
          onClick={() => {
            if (
              window.confirm(
                'Replace the recovery key? Download the new key before leaving this page.',
              )
            )
              void run(async () => setKey(await api.post('/recovery-key/regenerate')));
          }}
        >
          Regenerate recovery key
        </Button>
      </Panel>
      {key && (
        <RecoveryKeyStep
          busyLabel="Saving the new recovery key…"
          recoveryKey={key.recoveryKey}
          error={error}
          onDownloaded={async () => {
            try {
              await api.post('/recovery-key/confirm', { keyId: key.keyId });
              setKey(null);
              await q.refetch();
            } catch (e) {
              setError(e instanceof Error ? e.message : 'Could not confirm the key.');
            }
          }}
        />
      )}
    </div>
  );
}
