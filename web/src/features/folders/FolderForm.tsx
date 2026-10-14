import { useState } from 'react';
import { api } from '../../api/client.ts';
import { Button } from '../../components/Button.tsx';
import { Field } from '../../components/Field.tsx';
import { Select } from '../../components/Select.tsx';
import { Panel } from '../../components/Panel.tsx';
import { Notice } from '../../components/Notice.tsx';
export type Mode = 'LocalOnly' | 'CloudMount' | 'HybridBackup' | 'TieredStorage';
export type Folder = {
  id: string;
  generation: number;
  identity: { name: string; fileDockPath: string };
  storageMode: Mode;
  reference: {
    poolId?: string;
    relativePath?: string;
    connectionId?: string;
    remotePath?: string;
    backupDestination?: unknown;
  };
  dataImportance: string | null;
  access: string[];
  advanced: Record<string, unknown>;
  status?: { state: string; detail: string } | null;
  protectionState: string;
  exposure: string;
};
export type Options = {
  modes: { id: Mode; enabled: boolean; missingCapabilities: string[] }[];
  pools: { id: string; name: string; kind: string; status?: { state: string } | null }[];
  connections: { id: string; name: string }[];
  access: { id: string; enabled: boolean; missingCapability?: string }[];
};
const labels: Record<Mode, string> = {
  LocalOnly: 'Local Only',
  CloudMount: 'Cloud Mount',
  HybridBackup: 'Hybrid Backup',
  TieredStorage: 'Tiered Storage',
};
const defaults = (importance: string) => ({
  backupEnabled: importance !== 'Rebuildable',
  backupIntervalSeconds: importance === 'Critical' ? 300 : importance === 'Important' ? 900 : 3600,
  retention: { keepLast: 30 },
  subfolderImportance: {},
});
export function FolderForm({
  folder,
  options,
  canMutate,
  onSaved,
  onCancel,
}: {
  folder?: Folder;
  options: Options;
  canMutate: boolean;
  onSaved: () => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(folder?.identity.name ?? '');
  const [path, setPath] = useState(folder?.identity.fileDockPath ?? '/storage/');
  const [customPath, setCustomPath] = useState(!!folder);
  const [mode, setMode] = useState<Mode>(folder?.storageMode ?? 'LocalOnly');
  const [pool, setPool] = useState(folder?.reference.poolId ?? '');
  const [relative, setRelative] = useState(folder?.reference.relativePath ?? '');
  const [connection, setConnection] = useState(folder?.reference.connectionId ?? '');
  const [remote, setRemote] = useState(folder?.reference.remotePath ?? '');
  const destination = (folder?.reference.backupDestination ?? {}) as {
    poolId?: string;
    relativePath?: string;
    connectionId?: string;
    remotePath?: string;
  };
  const [backupType, setBackupType] = useState(destination.poolId ? 'local' : 'cloud');
  const [backupPool, setBackupPool] = useState(destination.poolId ?? '');
  const [backupConnection, setBackupConnection] = useState(destination.connectionId ?? '');
  const [backupPath, setBackupPath] = useState(
    destination.remotePath ?? destination.relativePath ?? '',
  );
  const [importance, setImportance] = useState(folder?.dataImportance ?? '');
  const [access, setAccess] = useState(folder?.access ?? []);
  const [advanced, setAdvanced] = useState(folder?.advanced ?? {});
  const [overrides, setOverrides] = useState<{ path: string; importance: string }[]>(
    Object.entries((folder?.advanced.subfolderImportance ?? {}) as Record<string, string>).map(
      ([path, importance]) => ({ path, importance }),
    ),
  );
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [issues, setIssues] = useState<{ severity: string; code: string; message: string }[]>([]);
  const local = mode !== 'CloudMount';
  const cloud = mode === 'CloudMount' || mode === 'TieredStorage';
  const warning = mode === 'LocalOnly' || mode === 'TieredStorage';
  const selectedPool = options.pools.find((p) => p.id === pool);
  const rank = (s: string | null | undefined) =>
    s === 'Critical' ? 3 : s === 'Important' ? 2 : s === 'Rebuildable' ? 1 : 0;
  const reducing =
    folder?.storageMode === 'HybridBackup' &&
    (mode !== 'HybridBackup' ||
      rank(importance) < rank(folder.dataImportance) ||
      Object.entries((folder.advanced.subfolderImportance ?? {}) as Record<string, string>).some(
        ([path, old]) =>
          rank(overrides.find((r) => r.path === path)?.importance ?? importance) < rank(old),
      ));
  async function save() {
    setError('');
    setIssues([]);
    setBusy(true);
    try {
      const body = {
        identity: { name, fileDockPath: path },
        storageMode: mode,
        reference: {
          ...(local ? { poolId: pool, relativePath: relative } : {}),
          ...(cloud ? { connectionId: connection, remotePath: remote } : {}),
          ...(mode === 'HybridBackup'
            ? {
                backupDestination:
                  backupType === 'cloud'
                    ? { connectionId: backupConnection, remotePath: backupPath }
                    : { poolId: backupPool, relativePath: backupPath },
              }
            : {}),
        },
        dataImportance: mode === 'HybridBackup' ? importance : null,
        access,
        advanced:
          mode === 'LocalOnly'
            ? {}
            : mode === 'HybridBackup'
              ? {
                  ...advanced,
                  subfolderImportance: Object.fromEntries(
                    overrides.map((r) => [r.path, r.importance]),
                  ),
                }
              : advanced,
        confirmImportanceReduction: confirm,
        confirmModeChange: confirm,
      };
      const report = await api.post<{
        valid: boolean;
        issues: { severity: string; code: string; message: string }[];
      }>(`/folders/validate${folder ? `?id=${encodeURIComponent(folder.id)}` : ''}`, body);
      setIssues(report.issues);
      if (!report.valid) return;
      if (folder) await api.patch(`/folders/${folder.id}`, body, String(folder.generation));
      else await api.post('/folders', body);
      onSaved();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <form
      className="stack"
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      {error ? <Notice tone="danger">{error}</Notice> : null}
      {issues.map((i, n) => (
        <Notice key={n} tone={i.severity === 'Warning' ? 'warning' : 'danger'}>
          {i.message} <span className="caption">({i.code})</span>
        </Notice>
      ))}
      <Panel className="stack">
        <h2>1. Identity</h2>
        <Field
          label="Name"
          required
          maxLength={64}
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            if (!customPath)
              setPath(
                '/storage/' +
                  e.target.value
                    .toLowerCase()
                    .replace(/[^a-z0-9_-]/g, '-')
                    .replace(/^-+|-+$/g, ''),
              );
          }}
        />
        <Field
          label="FileDock Path"
          required
          value={path}
          onChange={(e) => {
            setCustomPath(true);
            setPath(e.target.value);
          }}
          hint="A unique path under /storage. Managed folders cannot be nested."
        />
      </Panel>
      <Panel className="stack">
        <h2>2. Storage Mode</h2>
        <Select
          label="Storage mode"
          value={mode}
          onChange={(e) => {
            const m = e.target.value as Mode;
            setMode(m);
            setImportance('');
            setAdvanced({});
            setConfirm(false);
          }}
        >
          {options.modes.map((m) => (
            <option
              key={m.id}
              value={m.id}
              disabled={
                !m.enabled ||
                (!!folder && (folder.storageMode === 'CloudMount') !== (m.id === 'CloudMount'))
              }
            >
              {labels[m.id]}
              {m.enabled ? '' : ` — missing ${m.missingCapabilities.join(', ')}`}
            </option>
          ))}
        </Select>
        <p className="muted">The selected mode applies to this folder and every subfolder.</p>
        {warning ? (
          <Notice tone="warning">
            Data kept only on this appliance has no off-appliance copy and may be lost if its pool
            fails. Choose Hybrid Backup for data that needs backup.
          </Notice>
        ) : null}
      </Panel>
      <Panel className="stack">
        <h2>3. Source / Backend</h2>
        {local ? (
          <>
            <Select
              label="Pool"
              required
              value={pool}
              disabled={!!folder?.reference.poolId}
              onChange={(e) => setPool(e.target.value)}
            >
              <option value="">Choose an available pool</option>
              {options.pools.map((p) => (
                <option
                  key={p.id}
                  value={p.id}
                  disabled={
                    p.kind === 'System' &&
                    !['Available', 'Healthy', 'Ready'].includes(p.status?.state ?? '')
                  }
                >
                  {p.name}
                  {p.kind === 'System' ? ' (System Pool)' : ''}
                </option>
              ))}
            </Select>
            <Field
              label="Path inside pool"
              required
              value={relative}
              disabled={!!folder?.reference.poolId}
              onChange={(e) => setRelative(e.target.value)}
              placeholder="/movies"
              hint="A directory below the pool root. The root and /.filedock are reserved."
            />
            {selectedPool?.kind === 'System' ? (
              <Notice tone="warning">
                This folder shares the operating-system disk. Disk failure or an erased reinstall
                loses local data. Prefer a data pool for large or unprotected data.
              </Notice>
            ) : null}
            {!options.pools.length ? (
              <Notice tone="info">
                No pools are available. Pool creation is delivered in the storage phase.
              </Notice>
            ) : null}
          </>
        ) : null}
        {cloud ? (
          <>
            <Select
              label="Cloud connection"
              required
              value={connection}
              onChange={(e) => setConnection(e.target.value)}
            >
              <option value="">Choose a connection</option>
              {options.connections.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </Select>
            <Field
              label="Cloud reference path"
              required
              value={remote}
              onChange={(e) => setRemote(e.target.value)}
            />
          </>
        ) : null}
        {mode === 'HybridBackup' ? (
          <div className="stack">
            <Select
              label="Backup destination"
              value={backupType}
              onChange={(e) => setBackupType(e.target.value)}
            >
              <option value="cloud">Cloud connection</option>
              <option value="local">Another local pool path</option>
            </Select>
            {backupType === 'cloud' ? (
              <Select
                label="Backup connection"
                required
                value={backupConnection}
                onChange={(e) => setBackupConnection(e.target.value)}
              >
                <option value="">Choose a connection</option>
                {options.connections.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </Select>
            ) : (
              <Select
                label="Backup pool"
                required
                value={backupPool}
                onChange={(e) => setBackupPool(e.target.value)}
              >
                <option value="">Choose a pool</option>
                {options.pools.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </Select>
            )}
            <Field
              label="Backup destination path"
              required
              value={backupPath}
              onChange={(e) => setBackupPath(e.target.value)}
              hint="Must not overlap any managed folder reference."
            />
            {backupType === 'local' ? (
              <Notice tone="warning">
                A backup on this appliance does not protect against loss of the whole appliance. Use
                an off-appliance destination for machine-loss protection.
              </Notice>
            ) : null}
          </div>
        ) : null}
      </Panel>
      {mode === 'HybridBackup' ? (
        <Panel className="stack">
          <h2>4. Data Importance</h2>
          <Select
            label="Data importance"
            required
            value={importance}
            onChange={(e) => {
              setImportance(e.target.value);
              setAdvanced(defaults(e.target.value));
              setConfirm(false);
            }}
          >
            <option value="">Choose explicitly</option>
            <option value="Critical">Critical — back up every 5 minutes; restore first</option>
            <option value="Important">Important — back up every 15 minutes</option>
            <option value="Rebuildable">
              Rebuildable — no backup by default; reconnect or rebuild after loss
            </option>
          </Select>
          <p>
            Changes after the last durable recovery point may be lost. Only Temporary subfolder data
            may be deleted automatically under pressure.
          </p>
        </Panel>
      ) : null}
      <Panel className="stack">
        <h2>5. Access</h2>
        <div className="form-grid">
          {options.access.map((a) => (
            <label key={a.id}>
              <input
                type="checkbox"
                disabled={!a.enabled}
                checked={access.includes(a.id)}
                onChange={(e) =>
                  setAccess(e.target.checked ? [...access, a.id] : access.filter((v) => v !== a.id))
                }
              />{' '}
              {a.id}
              {a.enabled ? '' : ` — missing ${a.missingCapability}`}
            </label>
          ))}
        </div>
        <p className="muted">
          Selecting a protocol creates its export. Share users and export rules are managed
          separately. Docker permits application bindings.
        </p>
      </Panel>
      <Panel>
        <details>
          <summary>6. Advanced</summary>
          <div className="stack">
            {mode === 'LocalOnly' ? (
              <p>Local Only has no Advanced settings.</p>
            ) : mode === 'HybridBackup' ? (
              <>
                <label>
                  <input
                    type="checkbox"
                    checked={advanced.backupEnabled === true}
                    disabled={importance !== 'Rebuildable'}
                    onChange={(e) => setAdvanced({ ...advanced, backupEnabled: e.target.checked })}
                  />{' '}
                  {importance === 'Rebuildable'
                    ? 'Back up Rebuildable data (optional)'
                    : 'Backup is required for this importance'}
                </label>
                <Field
                  label="Backup interval (seconds)"
                  type="number"
                  min={1}
                  max={importance === 'Critical' ? 900 : 86400}
                  value={Number(
                    advanced.backupIntervalSeconds ??
                      (importance === 'Critical' ? 300 : importance === 'Important' ? 900 : 3600),
                  )}
                  onChange={(e) =>
                    setAdvanced({ ...advanced, backupIntervalSeconds: Number(e.target.value) })
                  }
                />
                <Field
                  label="Recovery points to retain"
                  type="number"
                  min={1}
                  value={Number((advanced.retention as { keepLast?: number })?.keepLast ?? 30)}
                  onChange={(e) =>
                    setAdvanced({ ...advanced, retention: { keepLast: Number(e.target.value) } })
                  }
                />
                <div className="stack">
                  <h3>Subfolder importance</h3>
                  <p className="muted">
                    Subfolders inherit the nearest parent importance unless an override is set.
                  </p>
                  {overrides.map((row, index) => (
                    <div className="form-grid" key={index}>
                      <Field
                        required
                        label={`Subfolder path ${index + 1}`}
                        value={row.path}
                        onChange={(e) =>
                          setOverrides(
                            overrides.map((r, i) =>
                              i === index ? { ...r, path: e.target.value } : r,
                            ),
                          )
                        }
                      />
                      <Select
                        required
                        label={`Subfolder importance ${index + 1}`}
                        value={row.importance}
                        onChange={(e) =>
                          setOverrides(
                            overrides.map((r, i) =>
                              i === index ? { ...r, importance: e.target.value } : r,
                            ),
                          )
                        }
                      >
                        <option value="">Choose importance</option>
                        {['Critical', 'Important', 'Rebuildable', 'Temporary'].map((v) => (
                          <option key={v}>{v}</option>
                        ))}
                      </Select>
                      <Button
                        variant="secondary"
                        onClick={() => setOverrides(overrides.filter((_, i) => i !== index))}
                      >
                        Remove override
                      </Button>
                    </div>
                  ))}
                  <Button
                    variant="secondary"
                    onClick={() => setOverrides([...overrides, { path: '', importance: '' }])}
                  >
                    Add subfolder override
                  </Button>
                </div>
              </>
            ) : (
              <div className="stack">
                {mode === 'CloudMount' ? (
                  <>
                    <Select
                      label="Cache profile"
                      value={String((advanced.cache as { profile?: string })?.profile ?? 'Generic')}
                      onChange={(e) =>
                        setAdvanced({
                          ...advanced,
                          cache: { ...((advanced.cache as object) ?? {}), profile: e.target.value },
                        })
                      }
                    >
                      {['Generic', 'Media', 'Aggressive Media'].map((v) => (
                        <option key={v}>{v}</option>
                      ))}
                    </Select>
                    <Field
                      label="Cache capacity (GB)"
                      type="number"
                      min={1}
                      value={Number((advanced.cache as { capacityGb?: number })?.capacityGb ?? 100)}
                      onChange={(e) =>
                        setAdvanced({
                          ...advanced,
                          cache: {
                            ...((advanced.cache as object) ?? {}),
                            capacityGb: Number(e.target.value),
                          },
                        })
                      }
                    />
                    <Select
                      label="Consistency"
                      value={String(advanced.consistency ?? 'Balanced')}
                      onChange={(e) => setAdvanced({ ...advanced, consistency: e.target.value })}
                    >
                      {['Aggressive', 'Balanced', 'Fresh'].map((v) => (
                        <option key={v}>{v}</option>
                      ))}
                    </Select>
                  </>
                ) : (
                  <>
                    <Field
                      label="Cold threshold (days without access)"
                      type="number"
                      min={1}
                      value={Number((advanced.tiering as { coldDays?: number })?.coldDays ?? 30)}
                      onChange={(e) =>
                        setAdvanced({
                          ...advanced,
                          tiering: {
                            ...((advanced.tiering as object) ?? {}),
                            coldDays: Number(e.target.value),
                          },
                        })
                      }
                    />
                    <Field
                      label="Hot-tier capacity (GB)"
                      type="number"
                      min={1}
                      value={Number(
                        (advanced.tiering as { capacityGb?: number })?.capacityGb ?? 500,
                      )}
                      onChange={(e) =>
                        setAdvanced({
                          ...advanced,
                          tiering: {
                            ...((advanced.tiering as object) ?? {}),
                            capacityGb: Number(e.target.value),
                          },
                        })
                      }
                    />
                    <p className="muted">
                      The tiering provider must verify cloud durability before moving data out of
                      the hot tier.
                    </p>
                  </>
                )}
              </div>
            )}
          </div>
        </details>
      </Panel>
      {reducing ? (
        <label>
          <input type="checkbox" checked={confirm} onChange={(e) => setConfirm(e.target.checked)} />{' '}
          I understand that lowering or removing importance permits loss of data that previously
          required backup.
        </label>
      ) : null}
      <div className="actions">
        <Button
          type="submit"
          disabled={
            !canMutate ||
            busy ||
            (!!reducing && !confirm) ||
            (mode === 'HybridBackup' && !importance)
          }
          busy={busy}
        >
          Save folder
        </Button>
        <Button variant="secondary" disabled={busy} onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
