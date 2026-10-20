import { RecoveryPoint } from '../../components/RecoveryPoint.tsx';
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
export function ExtensionsPage({ canMutate = true }: { canMutate?: boolean }) {
  const q = useQuery({
    queryKey: ['subsystems'],
    queryFn: () =>
      api.get<{ subsystems: Subsystem[]; defaultProviders: Record<string, string> }>('/subsystems'),
    refetchInterval: 5000,
  });
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
        action={
          <a className="button primary" href="#/extensions/package">
            Install extension
          </a>
        }
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
        <h2>Subsystems</h2>
        {q.data?.subsystems.map((s) => (
          <article className="resource-row extension-row" key={s.subsystemId}>
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
                {/* The subsystem DTO reports policy, not a durable backup observation. */}
                <RecoveryPoint
                  label="Latest durable bound-data recovery point"
                  cached={Boolean(q.error)}
                />
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
                  size="compact"
                  disabled={!canMutate || busy}
                  onClick={() => {
                    window.location.hash = `#/extensions/${encodeURIComponent(s.subsystemId)}/upgrade`;
                  }}
                >
                  Choose upgrade package
                </Button>
                <Button
                  variant="danger"
                  size="compact"
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
        footer={
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
        }
      >
        <p>
          Remove {remove?.name}? Its bound folders and their data will be kept. Removal is refused
          while folders, apps, or connector routes depend on it.
        </p>
      </Dialog>
    </div>
  );
}
