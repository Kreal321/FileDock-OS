import { useEffect, useState, type FormEvent } from 'react';
import { createApiClient } from '../../api/client.ts';
import { Panel } from '../../components/Panel.tsx';
import { Notice } from '../../components/Notice.tsx';
import { Field } from '../../components/Field.tsx';
import { Button } from '../../components/Button.tsx';
const api = createApiClient({ notifySessionExpiry: false });
type Detected = {
  applianceId: string;
  applianceName: string;
  newestSnapshotAt?: string;
  foundOn: string[];
  localSourceToken: string;
};
type Snapshot = {
  snapshotId: string;
  takenAt: string;
  integrityOk?: boolean;
  newestValid?: boolean;
};
export function RestoreWizard({ onDone, onCancel }: { onDone: () => void; onCancel: () => void }) {
  const [detected, setDetected] = useState<Detected[]>([]);
  const [snapshots, setSnapshots] = useState<Snapshot[] | null>(null);
  const [selected, setSelected] = useState('');
  const [mode, setMode] = useState('local');
  const [operation, setOperation] = useState('');
  const [status, setStatus] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    void api
      .get<{ appliances?: Detected[] }>('/setup/restore/local')
      .then((r) => setDetected(r.appliances ?? []))
      .catch((e) => setError(e.message));
  }, []);
  useEffect(() => {
    if (!operation) return;
    let disposed = false;
    const timer = setInterval(() => {
      void api
        .get<{
          state: string;
          steps?: { name: string; state: string; detail?: string }[];
          error?: string;
        }>(`/setup/restore/operations/${operation}`)
        .then((op) => {
          if (disposed) return;
          setStatus(op.state);
          if (op.state === 'FAILED' || op.state === 'Failed') {
            setError(op.error ?? 'Restore failed.');
            clearInterval(timer);
          }
          if (op.steps?.some((s) => s.name === 'import-management' && s.state === 'done')) {
            clearInterval(timer);
            onDone();
          }
        })
        .catch((e) => {
          if (!disposed) setError(e.message);
        });
    }, 2000);
    return () => {
      disposed = true;
      clearInterval(timer);
    };
  }, [operation, onDone]);
  async function open(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError('');
    const element = e.currentTarget;
    const form = new FormData(element);
    const recoveryKey = String(form.get('key') ?? '');
    try {
      if (mode === 'local') {
        await api.post('/setup/restore/local', {
          localSourceToken: String(form.get('source') ?? ''),
          recoveryKey,
        });
      } else if (mode === 'file') {
        const file = form.get('file');
        if (!(file instanceof File) || file.size === 0 || file.size > 128 * 1024 * 1024)
          throw new Error('Select a .fdbackup file smaller than 128 MiB.');
        const body = new FormData();
        body.append('file', file);
        body.append('setupCode', String(form.get('code') ?? ''));
        body.append('recoveryKey', recoveryKey);
        const response = await fetch('/api/v1/setup/restore/file', {
          method: 'POST',
          body,
          credentials: 'same-origin',
        });
        if (!response.ok) {
          const problem = await response.json();
          throw new Error(problem.detail ?? 'Backup file could not be opened.');
        }
      } else if (mode === 'disk') {
        await api.post('/setup/restore/source', {
          setupCode: String(form.get('code') ?? ''),
          recoveryKey,
          removableDiskPath: String(form.get('path') ?? ''),
        });
      } else {
        await api.post('/setup/restore/source', {
          setupCode: String(form.get('code') ?? ''),
          recoveryKey,
          providerFamily: mode === 'azure' ? 'azure-blob' : 's3-compatible',
          objectStorage: {
            connectionId: '',
            bucket: String(form.get('bucket') ?? ''),
            prefix: String(form.get('prefix') ?? ''),
          },
          connectionDefinition:
            mode === 'azure'
              ? { account: String(form.get('account') ?? '') }
              : {
                  endpoint: String(form.get('endpoint') ?? ''),
                  region: String(form.get('region') ?? 'us-east-1'),
                  addressingStyle: 'Path',
                },
          credentials:
            mode === 'azure'
              ? { key: String(form.get('secret') ?? '') }
              : {
                  accessKey: String(form.get('access') ?? ''),
                  secretKey: String(form.get('secret') ?? ''),
                },
        });
      }
      element.reset();
      const result = await api.get<{ snapshots?: Snapshot[] }>('/setup/restore/snapshots');
      setSnapshots(result.snapshots ?? []);
      setSelected(result.snapshots?.find((s) => s.newestValid)?.snapshotId ?? '');
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Source could not be opened.');
    } finally {
      setBusy(false);
    }
  }
  async function begin() {
    if (!selected || busy) return;
    setBusy(true);
    setError('');
    try {
      const op = await api.post<{ id: string }>('/setup/restore', { snapshotId: selected });
      setOperation(op.id);
      setStatus('Accepted');
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Restore could not start.');
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="stack">
      <h1>Restore an appliance</h1>
      <Notice>
        Configuration, credentials and identity are restored. File data recovery depends on the
        surviving pools and storage features. Changes after the selected snapshot are lost.
      </Notice>
      {error && (
        <Notice tone="danger" role="alert">
          {error}
        </Notice>
      )}
      {operation ? (
        <Panel className="stack">
          <h2>Restore {status}</h2>
          <p>Operation {operation}</p>
          <p>
            Importing the saved administrators. Sign in with the restored credentials once ready.
          </p>
        </Panel>
      ) : snapshots ? (
        <Panel className="stack">
          <h2>Choose a verified recovery point</h2>
          {snapshots.map((s) => (
            <label key={s.snapshotId}>
              <input
                type="radio"
                name="snapshot"
                checked={selected === s.snapshotId}
                disabled={!s.integrityOk}
                onChange={() => setSelected(s.snapshotId)}
              />
              {new Date(s.takenAt).toLocaleString()} · {s.integrityOk ? 'Verified' : 'Invalid'}
              {s.newestValid ? ' · Newest valid' : ''}
            </label>
          ))}
          {!snapshots.length && <p>No valid snapshots found.</p>}
          <Button variant="primary" disabled={!selected} busy={busy} onClick={() => void begin()}>
            Restore selected snapshot
          </Button>
        </Panel>
      ) : (
        <form className="panel stack" onSubmit={open}>
          <label>
            Recovery source
            <select value={mode} onChange={(e) => setMode(e.target.value)}>
              <option value="local">Found on this appliance</option>
              <option value="file">Configuration backup file</option>
              <option value="disk">Removable disk</option>
              <option value="s3">S3-compatible object storage</option>
              <option value="azure">Azure Blob Storage</option>
            </select>
          </label>
          {mode === 'local' ? (
            <>
              <label>
                Detected appliance
                <select name="source" required>
                  {detected.map((d) => (
                    <option key={d.localSourceToken} value={d.localSourceToken}>
                      {d.applianceName} · {d.newestSnapshotAt ?? 'unknown time'} ·{' '}
                      {d.foundOn.join(', ')}
                    </option>
                  ))}
                </select>
              </label>
              {!detected.length && (
                <p>No local copy detected. Choose a backup file or remote source.</p>
              )}
            </>
          ) : (
            <Field label="One-time setup code" name="code" required autoComplete="one-time-code" />
          )}
          {mode === 'file' && (
            <Field
              label="Configuration backup (.fdbackup)"
              name="file"
              type="file"
              accept=".fdbackup"
              required
            />
          )}
          {mode === 'disk' && <Field label="Mounted removable disk path" name="path" required />}
          {mode === 'azure' && (
            <>
              <Field label="Account name" name="account" required />
              <Field label="Container" name="bucket" required />
              <Field label="Prefix" name="prefix" />
              <Field label="Account key" name="secret" type="password" required />
            </>
          )}
          {mode === 's3' && (
            <>
              <Field label="Endpoint" name="endpoint" required />
              <Field label="Region" name="region" defaultValue="us-east-1" />
              <Field label="Bucket" name="bucket" required />
              <Field label="Prefix" name="prefix" />
              <Field label="Access key" name="access" required />
              <Field label="Secret key" name="secret" type="password" required />
            </>
          )}
          <Field label="Recovery key" name="key" type="password" required autoComplete="off" />
          <Button
            variant="primary"
            type="submit"
            busy={busy}
            disabled={mode === 'local' && !detected.length}
          >
            Open recovery source
          </Button>
        </form>
      )}
      {!operation && <Button onClick={onCancel}>Back to first-run setup</Button>}
    </div>
  );
}
