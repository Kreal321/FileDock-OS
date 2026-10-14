import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api } from '../../api/client.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { Notice } from '../../components/Notice.tsx';
import { Button } from '../../components/Button.tsx';
import { Field } from '../../components/Field.tsx';
import { Dialog } from '../../components/Dialog.tsx';
type Subsystem = {
  subsystemId: string;
  name: string;
  version: string;
  class: string;
  runtimeType: string;
  state: string;
  capabilities: string[];
  dependentFolders: string[];
  dataBindings: Record<string, string>;
  dataProtected: boolean;
};
type Folder = { id: string; name: string; storageMode: string; protected: boolean };
type Inspection = {
  stagedPackageId: string;
  valid: boolean;
  signed: boolean;
  signerKeyId: string;
  manifest: { id: string; name: string; version: string };
  requestedPermissions: string[];
  issues: { message: string; code: string }[];
  dataPaths: {
    name: string;
    description: string;
    mountPath: string;
    suggestedImportance: string;
  }[];
};
export function ExtensionsPage({ canMutate = true }: { canMutate?: boolean }) {
  const q = useQuery({
    queryKey: ['subsystems'],
    queryFn: () =>
      api.get<{ subsystems: Subsystem[]; defaultProviders: Record<string, string> }>('/subsystems'),
    refetchInterval: 5000,
  });
  const folders = useQuery({
    queryKey: ['extension-folders'],
    queryFn: () => api.get<{ folders: Folder[] }>('/extensions/folders'),
  });
  const [inspection, setInspection] = useState<Inspection | null>(null);
  const [action, setAction] = useState<'install' | 'upgrade' | 'supply'>('install');
  const [selected, setSelected] = useState('');
  const [approved, setApproved] = useState<string[]>([]);
  const [unsigned, setUnsigned] = useState(false);
  const [unprotected, setUnprotected] = useState(false);
  const [bindings, setBindings] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [operation, setOperation] = useState('');
  const [remove, setRemove] = useState<Subsystem | null>(null);
  async function run(fn: () => Promise<void>) {
    if (!canMutate || busy) return;
    setBusy(true);
    setError('');
    try {
      await fn();
      await q.refetch();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Request failed.');
    } finally {
      setBusy(false);
    }
  }
  async function upload(file: File) {
    await run(async () => {
      setInspection(null);
      setApproved([]);
      setUnsigned(false);
      setUnprotected(false);
      setBindings({});
      const form = new FormData();
      form.append('file', file);
      const match = document.cookie.match(/(?:^|; )fd_http_csrf=([^;]*)/);
      const response = await fetch('/api/v1/extensions/inspect', {
        method: 'POST',
        body: form,
        credentials: 'same-origin',
        headers: { 'X-CSRF-Token': match ? decodeURIComponent(match[1]!) : '' },
      });
      const value = await response.json();
      if (!response.ok) throw new Error(value.detail ?? 'Upload failed.');
      setInspection(value);
    });
  }
  const unprotectedBinding = Object.values(bindings).some(
    (id) => !folders.data?.folders.find((f) => f.id === id)?.protected,
  );
  const ready =
    Boolean(inspection?.valid) &&
    (action === 'supply' ||
      (inspection!.requestedPermissions.every((p) => approved.includes(p)) &&
        (inspection!.signed || unsigned) &&
        (action === 'upgrade' ||
          (inspection!.dataPaths.every((p) => bindings[p.name]) &&
            (!unprotectedBinding || unprotected)))));
  async function install() {
    await run(async () => {
      if (!inspection || !ready) return;
      let result: { id: string };
      if (action === 'supply')
        throw new Error('Use the recorded-package upload control on the extension card.');
      if (action === 'upgrade') {
        result = await api.post(`/extensions/${encodeURIComponent(selected)}/upgrade`, {
          stagedPackageId: inspection.stagedPackageId,
          approvedPermissions: approved,
          acknowledgedUnsigned: unsigned,
        });
      } else {
        result = await api.post('/extensions', {
          stagedPackageId: inspection.stagedPackageId,
          approvedPermissions: approved,
          acknowledgedUnsigned: unsigned,
          dataBindings: bindings,
          acknowledgedUnprotectedData: unprotected,
        });
      }
      setOperation(result.id);
      setInspection(null);
    });
  }
  async function supply(id: string, file: File) {
    await run(async () => {
      const body = new FormData();
      body.append('file', file);
      const match = document.cookie.match(/(?:^|; )fd_http_csrf=([^;]*)/);
      const response = await fetch(`/api/v1/extensions/${encodeURIComponent(id)}/package`, {
        method: 'POST',
        body,
        credentials: 'same-origin',
        headers: { 'X-CSRF-Token': match ? decodeURIComponent(match[1]!) : '' },
      });
      const value = await response.json();
      if (!response.ok) throw new Error(value.detail ?? 'Recorded package was refused.');
      setOperation(value.id);
    });
  }
  const capabilities = [...new Set(q.data?.subsystems.flatMap((s) => s.capabilities) ?? [])].filter(
    (c) => c !== 'cloud.remote' && c !== 'cloud.mount',
  );
  return (
    <div className="stack">
      <PageHeader
        title="Extensions"
        description="Install capabilities outside the Core. Persistent data stays in bound managed folders."
      />
      {error && (
        <Notice tone="danger" role="alert">
          {error}
        </Notice>
      )}
      {q.isPending && <Panel>Loading subsystems…</Panel>}
      {q.error && (
        <Notice tone="warning">
          Subsystem status unavailable. {q.error.message}
          <Button onClick={() => void q.refetch()}>Retry</Button>
        </Notice>
      )}
      {operation && (
        <Notice tone="info">
          Operation accepted. Check its result before relying on this extension.{' '}
          <a href={`#/settings/operations/${operation}`}>View operation progress</a>
        </Notice>
      )}
      <Panel className="stack">
        <h2>{action === 'upgrade' ? `Upgrade ${selected}` : 'Install an extension'}</h2>
        <Field
          type="file"
          label="Extension package (.fdext)"
          accept=".fdext"
          disabled={!canMutate || busy}
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) void upload(file);
          }}
        />
        {action === 'upgrade' && (
          <Button
            variant="secondary"
            onClick={() => {
              setAction('install');
              setSelected('');
              setInspection(null);
            }}
          >
            Cancel upgrade
          </Button>
        )}
        {inspection && (
          <div className="stack">
            <h3>
              {inspection.manifest?.name} {inspection.manifest?.version}
            </h3>
            {!inspection.valid ? (
              <Notice tone="danger">
                {inspection.issues.map((i) => `${i.code}: ${i.message}`).join(' ')}
              </Notice>
            ) : (
              <>
                <Notice tone={inspection.signed ? 'success' : 'warning'}>
                  {inspection.signed
                    ? `Verified signature: ${inspection.signerKeyId}`
                    : 'Unsigned package. Its author has not been verified.'}
                </Notice>
                <h3>Requested permissions</h3>
                {inspection.requestedPermissions.length === 0 && (
                  <p>No optional permissions requested.</p>
                )}
                {inspection.requestedPermissions.map((permission) => (
                  <label key={permission}>
                    <input
                      type="checkbox"
                      checked={approved.includes(permission)}
                      onChange={(e) =>
                        setApproved(
                          e.target.checked
                            ? [...approved, permission]
                            : approved.filter((p) => p !== permission),
                        )
                      }
                      disabled={!canMutate || busy}
                    />{' '}
                    {permission}
                  </label>
                ))}
                {!inspection.signed && (
                  <label>
                    <input
                      type="checkbox"
                      checked={unsigned}
                      onChange={(e) => setUnsigned(e.target.checked)}
                      disabled={!canMutate || busy}
                    />{' '}
                    I accept installing an unsigned package.
                  </label>
                )}
                {inspection.dataPaths.length > 0 && (
                  <>
                    <h3>Persistent data</h3>
                    <Notice tone="info">
                      Recommended: bind each path to a Hybrid Backup managed folder at the suggested
                      importance. Folder protection must be configured and healthy before relying on
                      backups.
                    </Notice>
                    {action === 'upgrade' ? (
                      <p>
                        Existing bindings are retained. A package that adds or removes data paths is
                        refused.
                      </p>
                    ) : (
                      inspection.dataPaths.map((path) => (
                        <div className="field" key={path.name}>
                          <label htmlFor={`binding-${path.name}`}>
                            {path.name} · {path.mountPath}
                          </label>
                          <p>
                            {path.description} Suggested Hybrid Backup importance:{' '}
                            {path.suggestedImportance}.
                          </p>
                          <select
                            id={`binding-${path.name}`}
                            value={bindings[path.name] ?? ''}
                            onChange={(e) =>
                              setBindings({ ...bindings, [path.name]: e.target.value })
                            }
                            disabled={!canMutate || busy || Boolean(folders.error)}
                          >
                            <option value="">Choose a managed folder</option>
                            {(folders.data?.folders ?? []).map((folder) => (
                              <option value={folder.id} key={folder.id}>
                                {folder.name} · {folder.storageMode}
                                {folder.protected ? '' : ' · Unprotected'}
                              </option>
                            ))}
                          </select>
                        </div>
                      ))
                    )}
                    {!folders.data?.folders.length && action === 'install' && (
                      <Notice tone="warning">
                        No managed folders are available. Create a Hybrid Backup folder before
                        installing an extension that stores data.
                      </Notice>
                    )}
                    {unprotectedBinding && action === 'install' && (
                      <Notice tone="warning">
                        This binding is unprotected. Data can be lost with the appliance.
                        <label>
                          <input
                            type="checkbox"
                            checked={unprotected}
                            onChange={(e) => setUnprotected(e.target.checked)}
                            disabled={!canMutate || busy}
                          />{' '}
                          I accept the loss exposure of the selected folders.
                        </label>
                      </Notice>
                    )}
                  </>
                )}
                <Button disabled={!canMutate || !ready} busy={busy} onClick={() => void install()}>
                  {action === 'upgrade' ? 'Upgrade with health check' : 'Approve and install'}
                </Button>
              </>
            )}
          </div>
        )}
      </Panel>
      <Panel className="stack">
        <h2>Subsystems</h2>
        {q.data?.subsystems.map((s) => (
          <article className="panel stack" key={s.subsystemId}>
            <h3>
              {s.name}{' '}
              <span className="caption">
                {s.version} · {s.class} · {s.runtimeType}
              </span>
            </h3>
            <p>Status: {s.state}</p>
            <p>Capabilities: {s.capabilities.join(', ')}</p>
            {Object.keys(s.dataBindings).length > 0 && (
              <Notice tone={s.dataProtected ? 'info' : 'warning'}>
                {s.dataProtected
                  ? 'Bindings use protected folder policies.'
                  : 'Bound data is unprotected.'}{' '}
                {Object.entries(s.dataBindings)
                  .map(([name, id]) => `${name}: ${id}`)
                  .join(', ')}
              </Notice>
            )}
            {s.dependentFolders.length > 0 && (
              <p>Dependent folders: {s.dependentFolders.join(', ')}</p>
            )}
            {s.class === 'Extension' && (
              <div className="actions">
                {s.state !== 'Running' && s.state !== 'PackageNeeded' && (
                  <Button
                    disabled={!canMutate || busy}
                    onClick={() =>
                      void run(async () => {
                        const result = await api.post<{ id: string }>(
                          `/extensions/${encodeURIComponent(s.subsystemId)}/restart`,
                        );
                        setOperation(result.id);
                      })
                    }
                  >
                    Restart extension
                  </Button>
                )}
                <Button
                  variant="secondary"
                  disabled={!canMutate || busy}
                  onClick={() => {
                    setSelected(s.subsystemId);
                    setAction('upgrade');
                    setInspection(null);
                  }}
                >
                  Choose upgrade package
                </Button>
                <Button
                  variant="danger"
                  disabled={!canMutate || busy || s.dependentFolders.length > 0}
                  onClick={() => setRemove(s)}
                >
                  Remove
                </Button>
                {s.state === 'PackageNeeded' && (
                  <Field
                    type="file"
                    label={`Supply recorded package for ${s.name}`}
                    accept=".fdext"
                    disabled={!canMutate || busy}
                    onChange={(e) => {
                      const file = e.target.files?.[0];
                      if (file) void supply(s.subsystemId, file);
                    }}
                  />
                )}
              </div>
            )}
          </article>
        ))}
      </Panel>
      <Panel className="stack">
        <h2>Default capability providers</h2>
        <p>Cloud providers are selected through connector routes.</p>
        {capabilities.map((capability) => (
          <div className="field" key={capability}>
            <label htmlFor={`provider-${capability}`}>{capability}</label>
            <select
              id={`provider-${capability}`}
              value={q.data?.defaultProviders[capability] ?? ''}
              disabled={!canMutate || busy || Boolean(q.error)}
              onChange={(e) =>
                void run(async () => {
                  await api.put(`/capabilities/${encodeURIComponent(capability)}/default`, {
                    subsystemId: e.target.value,
                  });
                })
              }
            >
              <option value="">
                {q.data?.subsystems.filter((s) => s.capabilities.includes(capability)).length === 1
                  ? 'Only provider is used automatically'
                  : 'Choose a default provider'}
              </option>
              {q.data?.subsystems
                .filter((s) => s.capabilities.includes(capability))
                .map((s) => (
                  <option key={s.subsystemId} value={s.subsystemId}>
                    {s.name} · {s.state}
                  </option>
                ))}
            </select>
          </div>
        ))}
      </Panel>
      <Dialog
        open={Boolean(remove)}
        title="Remove extension"
        onClose={() => setRemove(null)}
        busy={busy}
      >
        <p>
          Remove {remove?.name}? Its bound folders and their data will be kept. Removal is refused
          while folders, apps, or connector routes depend on it.
        </p>
        <Button
          variant="danger"
          busy={busy}
          disabled={!canMutate}
          onClick={() =>
            void run(async () => {
              if (remove) {
                const result = await api.delete<{ id: string }>(
                  `/extensions/${encodeURIComponent(remove.subsystemId)}`,
                );
                setOperation(result.id);
                setRemove(null);
              }
            })
          }
        >
          Confirm removal
        </Button>
      </Dialog>
    </div>
  );
}
