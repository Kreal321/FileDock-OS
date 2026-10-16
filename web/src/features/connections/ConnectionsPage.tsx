import { useResourcePage } from '../resources/useResourcePage.tsx';
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { api } from '../../api/client.ts';
import { Panel } from '../../components/Panel.tsx';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Button } from '../../components/Button.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import { Field, TextAreaField } from '../../components/Field.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { RowActions } from '../../components/RowActions.tsx';
import { StateView } from '../../components/StateView.tsx';
import {
  useResourceAction,
  type Resource,
  type PageProps,
} from '../resources/useResourceAction.ts';
import { useUnsavedChanges } from '../../navigation/useUnsavedChanges.ts';
type Flow = {
  flowId: string;
  authUrl?: string;
  question?: string;
  complete?: boolean;
  credentialRef?: string;
};
type Options = {
  connectors: {
    subsystemId: string;
    enabled: boolean;
    families: {
      family: string;
      connectionSchema?: { properties?: Record<string, { title?: string; type?: string }> };
    }[];
  }[];
};
export function ConnectionsPage({ canMutate = true, connected = true }: PageProps) {
  const pagination = useResourcePage();
  const connections = useQuery({
    queryKey: ['connections', pagination.cursor],
    queryFn: () =>
      api.get<{ items: Resource[]; nextPageToken?: string | null }>(
        `/connections${pagination.query}`,
      ),
  });
  const options = useQuery({
    queryKey: ['connection-options'],
    queryFn: () => api.get<Options>('/connections/options'),
  });
  const action = useResourceAction(canMutate, connected, ['connections']);
  const [editing, setEditing] = useState<Resource | null | undefined>(undefined);
  const [removing, setRemoving] = useState<Resource | null>(null);
  const [name, setName] = useState('');
  const [family, setFamily] = useState('s3-compatible');
  const [endpoint, setEndpoint] = useState('');
  const [region, setRegion] = useState('us-east-1');
  const [bucket, setBucket] = useState('');
  const [addressing, setAddressing] = useState('Path');
  const [accessKey, setAccessKey] = useState('');
  const [secretKey, setSecretKey] = useState('');
  const [config, setConfig] = useState('{}');
  const [dirty, setDirty] = useState(false);
  const [flow, setFlow] = useState<Flow | null>(null);
  const [paste, setPaste] = useState('');
  const [test, setTest] = useState<unknown>(null);
  useUnsavedChanges(dirty);
  const families = Array.from(
    new Set(
      options.data?.connectors
        .filter((c) => c.enabled)
        .flatMap((c) => c.families.map((f) => f.family)) ?? [],
    ),
  );
  function open(row: Resource | null) {
    setEditing(row);
    setName(String(row?.spec.name ?? ''));
    setFamily(String(row?.spec.providerFamily ?? 's3-compatible'));
    const def = (row?.spec.definition ?? {}) as Record<string, unknown>;
    setEndpoint(String(def.endpoint ?? ''));
    setRegion(String(def.region ?? 'us-east-1'));
    setBucket(String(def.bucket ?? ''));
    setAddressing(String(def.addressingStyle ?? 'Path'));
    setConfig(JSON.stringify(def.config ?? {}, null, 2));
    setAccessKey('');
    setSecretKey('');
    setFlow(null);
    setPaste('');
    setDirty(false);
    action.setError('');
  }
  function close() {
    setEditing(undefined);
    setRemoving(null);
    setDirty(false);
    setAccessKey('');
    setSecretKey('');
    setFlow(null);
    setPaste('');
  }
  const schema = options.data?.connectors
    .find((c) => c.enabled && c.families.some((f) => f.family === family))
    ?.families.find((f) => f.family === family)?.connectionSchema;
  function configuration() {
    try {
      return JSON.parse(config) as Record<string, string>;
    } catch {
      return {};
    }
  }
  function save() {
    void action.act(() => {
      const old = (editing?.spec.definition ?? {}) as Record<string, unknown>;
      const definition =
        family === 's3-compatible'
          ? {
              type: 's3',
              endpoint,
              region,
              bucket,
              addressingStyle: addressing,
              ...(accessKey ? { accessKey } : { accessKeyRef: old.accessKeyRef }),
              ...(secretKey ? { secretKey } : { secretKeyRef: old.secretKeyRef }),
            }
          : {
              type: 'bound',
              connectorId: options.data?.connectors.find(
                (c) => c.enabled && c.families.some((f) => f.family === family),
              )?.subsystemId,
              config: JSON.parse(config),
              credentials: flow?.credentialRef ? [flow.credentialRef] : (old.credentials ?? []),
            };
      const spec = {
        name,
        providerFamily: family,
        definition,
        ...(editing?.spec.routeOverrides ? { routeOverrides: editing.spec.routeOverrides } : {}),
      };
      return editing
        ? api.patch(`/connections/${editing.id}`, spec, String(editing.generation))
        : api.post('/connections', spec);
    }, close);
  }
  return (
    <div className="stack resource-page">
      <PageHeader
        title="Cloud connections"
        description="Keep provider credentials in the encrypted vault. Each operation uses its routed connector."
        action={
          <Button disabled={action.disabled || !families.length} onClick={() => open(null)}>
            Add connection
          </Button>
        }
      />
      {action.error && editing === undefined && !removing ? (
        <p role="alert">{action.error}</p>
      ) : null}
      <Panel>
        <StateView
          phase={
            connections.isPending
              ? 'loading'
              : connections.error
                ? 'error'
                : connections.data?.items.length
                  ? 'ready'
                  : 'empty'
          }
          error={connections.error?.message}
          onRetry={() => void connections.refetch()}
          empty="No cloud connections yet."
        />
        {connections.data?.items.length ? (
          <DataTable
            label="Cloud connections"
            headers={['Connection', 'Provider', 'Last observation', 'Actions']}
          >
            {connections.data.items.map((r) => (
              <tr key={r.id}>
                <td>
                  <strong>{String(r.spec.name)}</strong>
                </td>
                <td>{String(r.spec.providerFamily)}</td>
                <td>
                  {r.status?.lastTest
                    ? String(
                        (r.status.lastTest as { connectorId?: string }).connectorId ?? 'Tested',
                      )
                    : 'Not tested'}
                </td>
                <td>
                  <RowActions
                    label={`Actions for ${r.spec.name}`}
                    actions={[
                      {
                        label: 'Test connection',
                        disabled: action.disabled,
                        onClick: () =>
                          void action.act(async () => {
                            const value = await api.post(`/connections/${r.id}/test`);
                            setTest(value);
                          }),
                      },
                      { label: 'Edit', disabled: action.disabled, onClick: () => open(r) },
                      {
                        label: 'Remove',
                        danger: true,
                        disabled: action.disabled,
                        onClick: () => setRemoving(r),
                      },
                    ]}
                  />
                </td>
              </tr>
            ))}
          </DataTable>
        ) : null}
      </Panel>
      {pagination.controls(connections.data?.nextPageToken)}
      <Dialog
        open={editing !== undefined}
        title={editing ? 'Edit cloud connection' : 'Add cloud connection'}
        onClose={close}
        dirty={dirty}
        busy={action.busy}
        footer={
          <Button disabled={action.disabled || !name} busy={action.busy} onClick={save}>
            Save connection
          </Button>
        }
      >
        <form
          className="stack"
          onChange={() => setDirty(true)}
          onSubmit={(e) => {
            e.preventDefault();
            save();
          }}
        >
          <Field label="Connection name" value={name} onChange={(e) => setName(e.target.value)} />
          <label>
            Provider family
            <select
              value={family}
              disabled={!!editing}
              onChange={(e) => {
                setFamily(e.target.value);
                setFlow(null);
              }}
            >
              {families.map((f) => (
                <option key={f}>{f}</option>
              ))}
            </select>
          </label>
          {family === 's3-compatible' ? (
            <>
              <Field
                label="Endpoint"
                type="url"
                placeholder="https://s3.example.com"
                value={endpoint}
                onChange={(e) => setEndpoint(e.target.value)}
              />
              <Field label="Region" value={region} onChange={(e) => setRegion(e.target.value)} />
              <Field label="Bucket" value={bucket} onChange={(e) => setBucket(e.target.value)} />
              <label>
                Addressing style
                <select value={addressing} onChange={(e) => setAddressing(e.target.value)}>
                  <option>Path</option>
                  <option>Virtual</option>
                </select>
              </label>
              <Field
                label="Access key"
                type="password"
                autoComplete="off"
                value={accessKey}
                onChange={(e) => setAccessKey(e.target.value)}
                hint={editing ? 'Leave empty to keep the stored key.' : undefined}
              />
              <Field
                label="Secret key"
                type="password"
                autoComplete="new-password"
                value={secretKey}
                onChange={(e) => setSecretKey(e.target.value)}
              />
              <p className="muted">Static keys only. Role/session credentials are not supported.</p>
            </>
          ) : (
            <>
              {Object.entries(schema?.properties ?? {}).map(([key, property]) => (
                <Field
                  key={key}
                  label={property.title ?? key}
                  value={configuration()[key] ?? ''}
                  onChange={(e) => {
                    setConfig(JSON.stringify({ ...configuration(), [key]: e.target.value }));
                    setDirty(true);
                  }}
                />
              ))}
              <details>
                <summary>Advanced connector configuration</summary>
                <TextAreaField
                  label="Connector configuration (JSON)"
                  rows={6}
                  value={config}
                  onChange={(e) => setConfig(e.target.value)}
                  hint="Provider configuration follows its connector schema. Store authorization tokens through Authorize, not in this field."
                />
              </details>
              <Button
                type="button"
                variant="secondary"
                disabled={action.disabled}
                onClick={() =>
                  void action.act(async () =>
                    setFlow(
                      await api.post<Flow>('/connections/oauth/start', {
                        providerFamily: family,
                        config: JSON.parse(config),
                      }),
                    ),
                  )
                }
              >
                Authorize provider
              </Button>
              {flow ? (
                <>
                  <p>
                    {flow.complete
                      ? 'Authorization complete. Save this connection to use it.'
                      : flow.question}
                  </p>
                  {flow.authUrl?.startsWith('https://') ? (
                    <a href={flow.authUrl} target="_blank" rel="noreferrer">
                      Open provider authorization
                    </a>
                  ) : null}
                  {!flow.complete ? (
                    <>
                      <TextAreaField
                        label="Paste authorization response"
                        rows={4}
                        value={paste}
                        onChange={(e) => setPaste(e.target.value)}
                      />
                      <Button
                        type="button"
                        disabled={action.disabled || !paste}
                        onClick={() =>
                          void action.act(async () => {
                            setFlow(
                              await api.post<Flow>('/connections/oauth/finish', {
                                flowId: flow.flowId,
                                redirectUrl: paste,
                              }),
                            );
                            setPaste('');
                          })
                        }
                      >
                        Continue authorization
                      </Button>
                    </>
                  ) : null}
                </>
              ) : null}
            </>
          )}
        </form>
        {action.error ? <p role="alert">{action.error}</p> : null}
      </Dialog>
      <Dialog
        open={!!removing}
        title={`Remove ${removing?.spec.name ?? 'connection'}?`}
        onClose={close}
        busy={action.busy}
        footer={
          <Button
            variant="danger"
            disabled={action.disabled}
            onClick={() =>
              removing &&
              void action.act(
                () => api.del(`/connections/${removing.id}`, String(removing.generation)),
                close,
              )
            }
          >
            Remove connection
          </Button>
        }
      >
        <p>Removal is refused while a folder or state destination uses this connection.</p>
        {action.error ? <p role="alert">{action.error}</p> : null}
      </Dialog>
      <Dialog open={test !== null} title="Connection test" onClose={() => setTest(null)}>
        <pre className="code">{JSON.stringify(test, null, 2)}</pre>
      </Dialog>
    </div>
  );
}
