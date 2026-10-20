import { useUnsavedChanges } from '../../navigation/useUnsavedChanges.ts';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { api } from '../../api/client.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { Notice } from '../../components/Notice.tsx';
import { Button } from '../../components/Button.tsx';
import { Field } from '../../components/Field.tsx';
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
export function ExtensionPackagePage({
  canMutate = true,
  id,
}: {
  canMutate?: boolean;
  id?: string;
}) {
  const q = useQuery({
    queryKey: ['subsystems'],
    queryFn: () => api.get<{ subsystems: unknown[] }>('/subsystems'),
    refetchInterval: 5000,
  });
  const folders = useQuery({
    queryKey: ['extension-folders'],
    queryFn: () => api.get<{ folders: Folder[] }>('/extensions/folders'),
  });
  const [inspection, setInspection] = useState<Inspection | null>(null);
  const action = id ? 'upgrade' : 'install';
  const selected = id ?? '';
  const [approved, setApproved] = useState<string[]>([]);
  const [unsigned, setUnsigned] = useState(false);
  const [unprotected, setUnprotected] = useState(false);
  const [bindings, setBindings] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [operation, setOperation] = useState('');
  const active = useRef(true);
  useUnsavedChanges(Boolean(inspection));
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  async function run(fn: () => Promise<void>) {
    if (!canMutate || busy) return;
    setBusy(true);
    setError('');
    try {
      await fn();
      await q.refetch();
    } catch (e) {
      if (active.current) setError(e instanceof Error ? e.message : 'Request failed.');
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
      if (active.current) setInspection(value);
    });
  }
  const unprotectedBinding = Object.values(bindings).some(
    (id) => !folders.data?.folders.find((f) => f.id === id)?.protected,
  );
  const ready =
    Boolean(inspection?.valid) &&
    inspection!.requestedPermissions.every((p) => approved.includes(p)) &&
    (inspection!.signed || unsigned) &&
    (action === 'upgrade' ||
      (inspection!.dataPaths.every((p) => bindings[p.name]) &&
        (!unprotectedBinding || unprotected)));
  async function install() {
    await run(async () => {
      if (!inspection || !ready) return;
      let result: { id: string };
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
      if (active.current) {
        setOperation(result.id);
        setInspection(null);
      }
    });
  }
  return (
    <div className="stack">
      <PageHeader
        title={id ? `Upgrade ${id}` : 'Install an extension'}
        description="Install capabilities outside the Core. Persistent data stays in bound managed folders."
      />
      <a href="#/extensions">Back to extensions</a>
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
    </div>
  );
}
