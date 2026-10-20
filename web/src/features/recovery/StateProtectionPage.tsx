import { Dialog } from '../../components/Dialog.tsx';
import { SettingsRow } from '../../components/SettingsRow.tsx';
import { useUnsavedChanges } from '../../navigation/useUnsavedChanges.ts';
import {
  saveDestinationDraft,
  takeDestinationDraft,
  clearDestinationDraft,
} from './destinationDraft.ts';
import { RecoveryPoint } from '../../components/RecoveryPoint.tsx';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useId, useRef, useState, type FormEvent } from 'react';
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
export function StateProtectionPage({ canMutate: permission = true }: { canMutate?: boolean }) {
  const q = useQuery({
    queryKey: ['state-protection'],
    queryFn: () => api.get<Protection>('/state-protection'),
    refetchInterval: 10000,
  });
  const canMutate = permission && !q.error;
  const [key, setKey] = useState<{ keyId: string; recoveryKey: string } | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [operation, setOperation] = useState('');
  const [returnDraft] = useState(takeDestinationDraft);
  const [connectionId, setConnectionId] = useState(returnDraft?.connection ?? '');
  const [dialog, setDialog] = useState<'destination' | 'key' | null>(
    returnDraft ? 'destination' : null,
  );
  const [dirty, setDirty] = useState(false);
  const formId = useId();
  const formRef = useRef<HTMLFormElement>(null);
  const active = useRef(true);
  useUnsavedChanges(dirty);
  useEffect(() => {
    active.current = true;
    const clear = () => {
      active.current = false;
      clearDestinationDraft();
      setKey(null);
      setDialog(null);
      setDirty(false);
    };
    window.addEventListener('filedock:session-expired', clear);
    return () => {
      active.current = false;
      window.removeEventListener('filedock:session-expired', clear);
    };
  }, []);
  async function run(action: () => Promise<void>) {
    if (busy || !canMutate) return;
    setBusy(true);
    setError('');
    try {
      await action();
      if (active.current) await q.refetch();
    } catch (e) {
      if (active.current) setError(e instanceof Error ? e.message : 'Request failed.');
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
      if (active.current) {
        setOperation(op.id);
        setDialog(null);
        setDirty(false);
      }
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
                    {copy.kind} · {q.error ? 'Cached' : copy.stale ? 'Stale' : 'Current'}
                  </p>
                  <RecoveryPoint
                    at={copy.lastSnapshotAt}
                    cached={Boolean(q.error) || Boolean(copy.stale)}
                  />
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
                if (active.current) {
                  setOperation(op.id);
                  setDialog(null);
                  setDirty(false);
                }
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
      <Panel>
        <SettingsRow
          title="Remote recovery destination"
          description="Choose a separate copy for machine-loss protection."
          action={
            <Button
              disabled={!canMutate || busy}
              onClick={() => {
                setError('');
                setDialog('destination');
              }}
            >
              Add destination
            </Button>
          }
        />
      </Panel>
      <Dialog
        open={dialog === 'destination'}
        title="Add recovery destination"
        description="Encrypted appliance configuration and credentials; folder data is protected separately."
        busy={busy}
        dirty={dirty}
        onClose={() => {
          setDialog(null);
          setDirty(false);
        }}
        footer={
          <Button form={formId} type="submit" busy={busy} disabled={!canMutate}>
            Check and add
          </Button>
        }
      >
        {error ? <p role="alert">{error}</p> : null}
        <form
          id={formId}
          ref={formRef}
          className="stack"
          onSubmit={destination}
          onChange={() => setDirty(true)}
        >
          <Field
            label="Mounted removable disk path"
            name="disk"
            placeholder="/mnt/recovery"
            defaultValue={returnDraft?.disk}
          />
          <p>Or select an existing object-storage connection:</p>
          <Field
            label="Connection ID"
            name="connection"
            value={connectionId}
            onChange={(e) => setConnectionId(e.target.value)}
          />
          <Field label="Bucket" name="bucket" defaultValue={returnDraft?.bucket} />
          <Field label="Dedicated empty prefix" name="prefix" defaultValue={returnDraft?.prefix} />
          <label>
            <input name="basic" type="checkbox" defaultChecked={returnDraft?.basic} /> I accept
            basic write-then-verify claim safety without conditional writes.
          </label>
          <label>
            <input name="third" type="checkbox" defaultChecked={returnDraft?.third} /> I trust the
            third-party connector serving this destination.
          </label>
          <Button
            variant="quiet"
            disabled={busy || !canMutate}
            onClick={() => {
              const data = new FormData(formRef.current!);
              saveDestinationDraft({
                disk: String(data.get('disk') ?? ''),
                connection: connectionId,
                bucket: String(data.get('bucket') ?? ''),
                prefix: String(data.get('prefix') ?? ''),
                basic: data.get('basic') === 'on',
                third: data.get('third') === 'on',
              });
              setDirty(false);
              setDialog(null);
              setTimeout(() => {
                window.location.hash = '#/settings/state-protection/connection';
              }, 0);
            }}
          >
            Create connection
          </Button>
        </form>
      </Dialog>
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
            setError('');
            setDialog('key');
          }}
        >
          Regenerate recovery key
        </Button>
      </Panel>
      <Dialog
        open={dialog === 'key'}
        title="Regenerate recovery key"
        onClose={() => setDialog(null)}
        busy={busy}
        footer={
          <Button
            variant="danger"
            busy={busy}
            disabled={!canMutate}
            onClick={() =>
              void run(async () => {
                const result = await api.post<{ keyId: string; recoveryKey: string }>(
                  '/recovery-key/regenerate',
                );
                if (active.current) {
                  setKey(result);
                  setDialog(null);
                }
              })
            }
          >
            Replace recovery key
          </Button>
        }
      >
        <p>Keep your old key for older snapshots. Download the new key before leaving.</p>
        {error ? <p role="alert">{error}</p> : null}
      </Dialog>
      <Dialog
        open={!!key}
        title="Download new recovery key"
        onClose={() => {}}
        busy={true}
        cancelLabel="Download the key to continue"
      >
        {key && (
          <RecoveryKeyStep
            busyLabel="Saving the new recovery key…"
            recoveryKey={key.recoveryKey}
            error={error}
            onDownloaded={async () => {
              try {
                await api.post('/recovery-key/confirm', { keyId: key.keyId });
                setKey(null);
                if (active.current) await q.refetch();
              } catch (e) {
                setError(e instanceof Error ? e.message : 'Could not confirm the key.');
              }
            }}
          />
        )}
      </Dialog>
      <a href="#/settings/locations">Review owned locations in Cloud</a>
    </div>
  );
}
