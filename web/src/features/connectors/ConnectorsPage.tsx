import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { api } from '../../api/client.ts';
import { Panel } from '../../components/Panel.tsx';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Button } from '../../components/Button.tsx';
import { RowActions } from '../../components/RowActions.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import { Notice } from '../../components/Notice.tsx';
import { StateView } from '../../components/StateView.tsx';
import {
  useResourceAction,
  type PageProps,
  type Resource,
} from '../resources/useResourceAction.ts';
import { useUnsavedChanges } from '../../navigation/useUnsavedChanges.ts';
type Target = { connectorId?: string; backupEngine?: boolean };
type Route = { family: string; use: string; connectionId: string; target: Target };
type Table = { generation: number; defaults: Route[]; overrides: Route[] };
type Connector = {
  subsystemId: string;
  class: string;
  state: string;
  enabled: boolean;
  families?: { family: string; capabilities?: string[]; conditionalWrites?: boolean }[];
  routedBy?: unknown[];
};
type Impact = {
  connectionIds?: string[];
  folderIds?: string[];
  lostFeatures?: string[];
  mountsRestarting?: string[];
  credentialRecipients?: Record<string, { connectionIds?: string[] }>;
  ownedLocations?: {
    bucket: string;
    prefix: string;
    claimSafety: string;
    servedByConnector: string;
  }[];
  requiresBasicClaimAck?: boolean;
  requiresThirdPartyOwnedStorageAck?: boolean;
  issues?: { code: string; message: string }[];
};
const uses: Record<string, string> = {
  OwnedObjectStorage: 'owned storage',
  BackupRepository: 'backups',
  CloudMount: 'cloud mount',
};
const featureLabels: Record<string, string> = {
  conditional_writes: 'Conditional writes',
  provider_checksums: 'Provider checksums',
};
const targetId = (target: Target) =>
  target.backupEngine ? 'backup-engine' : (target.connectorId ?? '');
export function ConnectorsPage({ canMutate = true, connected = true }: PageProps) {
  const connectors = useQuery({
    queryKey: ['connectors'],
    queryFn: () => api.get<{ connectors?: Connector[] }>('/connectors'),
  });
  const routes = useQuery({
    queryKey: ['connector-routes'],
    queryFn: () => api.get<Table>('/connector-routes'),
  });
  const connections = useQuery({
    queryKey: ['connections', 'route-options'],
    queryFn: async () => {
      const rows: Resource[] = [];
      let cursor = '';
      do {
        const page = await api.get<{ items: Resource[]; nextPageToken?: string }>(
          `/connections?pageSize=500${cursor ? `&pageToken=${encodeURIComponent(cursor)}` : ''}`,
        );
        rows.push(...page.items);
        cursor = page.nextPageToken ?? '';
      } while (cursor);
      return rows;
    },
  });
  const action = useResourceAction(canMutate, connected, [
    'connectors',
    'connector-routes',
    'connections',
    'locations',
    'alerts',
  ]);
  const [draft, setDraft] = useState<{
    route: Route;
    generation: number;
    removable: boolean;
  } | null>(null);
  const [selected, setSelected] = useState('');
  const [impact, setImpact] = useState<Impact | null>(null);
  const [basic, setBasic] = useState(false);
  const [thirdParty, setThirdParty] = useState(false);
  const [adding, setAdding] = useState(false);
  const [connectionId, setConnectionId] = useState('');
  const [usage, setUsage] = useState('OwnedObjectStorage');
  useUnsavedChanges(!!draft);
  const name = (id: string) => String(connections.data?.find((c) => c.id === id)?.spec.name ?? id);
  function open(route: Route, removable = false) {
    setDraft({ route, generation: routes.data!.generation, removable });
    setSelected(targetId(route.target));
    setImpact(null);
    setBasic(false);
    setThirdParty(false);
    action.setError('');
  }
  function close() {
    setDraft(null);
    setImpact(null);
    setBasic(false);
    setThirdParty(false);
  }
  function payload() {
    return {
      changes: [
        {
          ...draft!.route,
          target:
            selected === 'inherit'
              ? null
              : selected === 'backup-engine'
                ? { backupEngine: true }
                : { connectorId: selected },
        },
      ],
      expectedGeneration: draft!.generation,
      acknowledgedBasicClaimSafety: basic,
      acknowledgedThirdPartyOwnedStorage: thirdParty,
    };
  }
  const candidates = connectors.data?.connectors?.filter(
    (c) =>
      draft !== null &&
      c.enabled &&
      c.families?.some(
        (f) =>
          f.family === draft.route.family &&
          f.capabilities?.includes(
            draft.route.use === 'CloudMount' ? 'cloud.mount' : 'cloud.remote',
          ),
      ),
  );
  function table(rows: Route[], label: string) {
    return (
      <DataTable label={label} headers={['Provider / connection', 'Use', 'Connector', 'Actions']}>
        {rows.map((r) => (
          <tr key={`${r.family}:${r.use}:${r.connectionId}`}>
            <td>
              <strong>{r.connectionId ? name(r.connectionId) : r.family}</strong>
              {r.connectionId ? <p className="muted">{r.family} · Override</p> : null}
            </td>
            <td>{uses[r.use] ?? r.use}</td>
            <td>
              <strong>{targetId(r.target)}</strong>
              {r.use === 'OwnedObjectStorage' &&
              connectors.data?.connectors?.some(
                (c) =>
                  c.subsystemId === targetId(r.target) &&
                  c.families?.some((f) => f.family === r.family && !f.conditionalWrites),
              ) ? (
                <p>
                  <span className="badge warning">Basic claim safety</span>
                </p>
              ) : null}
              {connectors.data?.connectors?.find((c) => c.subsystemId === targetId(r.target))
                ?.class === 'Extension' ? (
                <p>
                  <span className="badge info">Third-party</span>
                </p>
              ) : null}
            </td>
            <td>
              <RowActions
                label={`Actions for ${r.connectionId ? name(r.connectionId) : r.family} ${uses[r.use] ?? r.use} ${r.connectionId ? 'override' : 'default'}`}
                actions={[
                  {
                    label: 'Change connector',
                    disabled: action.disabled,
                    onClick: () => open(r, !!r.connectionId),
                  },
                ]}
              />
            </td>
          </tr>
        ))}
      </DataTable>
    );
  }
  return (
    <section className="stack resource-page">
      <PageHeader
        title="Connectors & routes"
        description="Choose which connector handles each cloud use. Changes take effect for new sessions."
        action={
          <Button
            disabled={action.disabled || !connections.data?.length || !routes.data}
            onClick={() => setAdding(true)}
          >
            Add override
          </Button>
        }
      />
      {action.error && !draft ? (
        <Notice tone="danger" role="alert">
          {action.error}
        </Notice>
      ) : null}
      <Panel>
        <h2>Family defaults</h2>
        <p className="muted">Connections inherit these routes unless an override is set.</p>
        <StateView
          phase={routes.isPending ? 'loading' : routes.error ? 'error' : 'ready'}
          error={routes.error?.message}
          onRetry={() => void routes.refetch()}
        />
        {routes.data ? table(routes.data.defaults, 'Family defaults') : null}
      </Panel>
      <Panel>
        <h2>Connection overrides</h2>
        {routes.data?.overrides.length ? (
          table(routes.data.overrides, 'Connection overrides')
        ) : (
          <p className="muted">All connections inherit their family defaults.</p>
        )}
      </Panel>
      <Panel>
        <h2>Installed connectors</h2>
        <StateView
          phase={
            connectors.isPending
              ? 'loading'
              : connectors.error
                ? 'error'
                : !connectors.data?.connectors?.length
                  ? 'empty'
                  : 'ready'
          }
          empty="No connectors are registered."
          error={connectors.error?.message}
          onRetry={() => void connectors.refetch()}
        />
        {connectors.data?.connectors?.length ? (
          <DataTable
            label="Installed connectors"
            headers={['Connector', 'Providers', 'State', 'Actions']}
          >
            {connectors.data.connectors.map((c) => (
              <tr key={c.subsystemId}>
                <td>
                  <strong>{c.subsystemId}</strong>
                  <p className="muted">{c.class === 'Extension' ? 'Third-party' : 'Built-in'}</p>
                </td>
                <td>{c.families?.map((f) => f.family).join(', ')}</td>
                <td>{c.enabled ? c.state : 'Disabled'}</td>
                <td>
                  <Button
                    variant="quiet"
                    size="compact"
                    disabled={action.disabled || !!(c.enabled && c.routedBy?.length)}
                    onClick={() =>
                      void action.act(() =>
                        api.patch(`/connectors/${encodeURIComponent(c.subsystemId)}`, {
                          enabled: !c.enabled,
                        }),
                      )
                    }
                  >
                    {c.enabled ? 'Disable' : 'Enable'}
                  </Button>
                </td>
              </tr>
            ))}
          </DataTable>
        ) : null}
        <p className="muted">
          A routed connector cannot be disabled. Installing a connector does not change any routes.
        </p>
      </Panel>
      <Dialog
        open={adding}
        title="Add connection override"
        onClose={() => setAdding(false)}
        footer={
          <Button
            disabled={!connectionId || action.disabled}
            onClick={() => {
              const c = connections.data!.find((c) => c.id === connectionId)!;
              const family = String(c.spec.providerFamily);
              const route = routes.data!.defaults.find(
                (r) => r.family === family && r.use === usage,
              );
              if (!route) {
                action.setError('This provider does not support the selected use.');
                return;
              }
              setAdding(false);
              open({ ...route, connectionId }, true);
            }}
          >
            Choose connector
          </Button>
        }
      >
        <label>
          Connection
          <select value={connectionId} onChange={(e) => setConnectionId(e.target.value)}>
            <option value="">Select a connection</option>
            {connections.data?.map((c) => (
              <option key={c.id} value={c.id}>
                {String(c.spec.name)}
              </option>
            ))}
          </select>
        </label>
        <label>
          Use
          <select value={usage} onChange={(e) => setUsage(e.target.value)}>
            {Object.entries(uses).map(([id, label]) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        </label>
        {action.error ? <p role="alert">{action.error}</p> : null}
      </Dialog>
      <Dialog
        open={!!draft}
        title="Change connector route"
        description={
          draft
            ? `${draft.route.family} → ${uses[draft.route.use] ?? draft.route.use}${draft.route.connectionId ? ` · ${name(draft.route.connectionId)}` : ''}`
            : undefined
        }
        onClose={close}
        busy={action.busy}
        dirty={!!draft}
        footer={
          impact ? (
            <Button
              disabled={
                action.disabled ||
                !!impact.issues?.length ||
                (impact.requiresBasicClaimAck && !basic) ||
                (impact.requiresThirdPartyOwnedStorageAck && !thirdParty)
              }
              onClick={() => void action.act(() => api.put('/connector-routes', payload()), close)}
            >
              Apply route change
            </Button>
          ) : (
            <Button
              disabled={action.disabled || !selected}
              onClick={() =>
                void action.act(async () =>
                  setImpact(await api.post<Impact>('/connector-routes/preview', payload())),
                )
              }
            >
              Preview change
            </Button>
          )
        }
      >
        <label>
          Connector
          <select
            value={selected}
            onChange={(e) => {
              setSelected(e.target.value);
              setImpact(null);
              setBasic(false);
              setThirdParty(false);
              action.setError('');
            }}
          >
            {draft?.removable ? <option value="inherit">Inherit family default</option> : null}
            {selected &&
            !candidates?.some((c) => c.subsystemId === selected) &&
            selected !== 'inherit' &&
            selected !== 'backup-engine' ? (
              <option value={selected} disabled>
                {selected} (unavailable)
              </option>
            ) : null}
            {candidates?.map((c) => (
              <option key={c.subsystemId} value={c.subsystemId}>
                {c.subsystemId}
                {c.class === 'Extension' ? ' · Third-party' : ''}
              </option>
            ))}
            {draft?.route.use === 'BackupRepository' ? (
              <option value="backup-engine">backup-engine</option>
            ) : null}
          </select>
        </label>
        {impact ? (
          <>
            <h3>Change impact</h3>
            <p>
              {impact.connectionIds?.length ?? 0} connections · {impact.folderIds?.length ?? 0}{' '}
              folders · {impact.mountsRestarting?.length ?? 0} mounts restarting
            </p>
            {impact.connectionIds?.length ? (
              <ul>
                {impact.connectionIds.map((id) => (
                  <li key={id}>{name(id)}</li>
                ))}
              </ul>
            ) : (
              <p className="muted">No existing connections are affected.</p>
            )}
            {impact.folderIds?.length ? (
              <>
                <h3>Affected folders</h3>
                <ul>
                  {impact.folderIds.map((id) => (
                    <li key={id}>
                      <code>{id}</code>
                    </li>
                  ))}
                </ul>
              </>
            ) : null}
            {impact.mountsRestarting?.length ? (
              <>
                <h3>Mounts to restart</h3>
                <Notice tone="warning">
                  Cloud mount access is briefly interrupted while these mounts restart.
                </Notice>
                <ul>
                  {impact.mountsRestarting.map((id) => (
                    <li key={id}>
                      <code>{id}</code>
                    </li>
                  ))}
                </ul>
              </>
            ) : null}
            {impact.lostFeatures?.length ? (
              <Notice tone="warning">
                Features lost: {impact.lostFeatures.map((f) => featureLabels[f] ?? f).join(', ')}
              </Notice>
            ) : null}
            <h3>New credential recipients</h3>
            {Object.keys(impact.credentialRecipients ?? {}).length ? (
              <ul>
                {Object.entries(impact.credentialRecipients!).map(([id, rows]) => (
                  <li key={id}>
                    <strong>{id}</strong> — {rows.connectionIds?.map(name).join(', ')}
                  </li>
                ))}
              </ul>
            ) : (
              <p className="muted">No additional connector receives credentials.</p>
            )}
            {impact.ownedLocations?.map((l) => (
              <p key={`${l.bucket}/${l.prefix}`}>
                <strong>
                  {l.bucket}/{l.prefix}
                </strong>{' '}
                · {l.servedByConnector} ·{' '}
                {l.claimSafety === 'CONDITIONAL' ? 'Conditional' : 'Basic claim safety'}
              </p>
            ))}
            {impact.requiresBasicClaimAck ? (
              <Notice tone="warning">
                <label>
                  <input
                    type="checkbox"
                    checked={basic}
                    onChange={(e) => setBasic(e.target.checked)}
                  />{' '}
                  I accept basic claim safety: this connector cannot enforce conditional ownership
                  claims.
                </label>
              </Notice>
            ) : null}
            {impact.requiresThirdPartyOwnedStorageAck ? (
              <Notice tone="warning">
                <label>
                  <input
                    type="checkbox"
                    checked={thirdParty}
                    onChange={(e) => setThirdParty(e.target.checked)}
                  />{' '}
                  I trust this third-party connector with owned storage and its credentials.
                </label>
              </Notice>
            ) : null}
            {impact.issues?.map((i, n) => (
              <p role="alert" key={`${i.code}:${n}`}>
                {i.message || i.code}
              </p>
            ))}
          </>
        ) : (
          <p className="muted">
            Preview the affected resources and safety changes before applying.
          </p>
        )}
        {action.error ? (
          <Notice tone="danger" role="alert">
            {action.error}
            <p>
              For a generation conflict, close this dialog and refresh routes before trying again.
            </p>
          </Notice>
        ) : null}
      </Dialog>
    </section>
  );
}
