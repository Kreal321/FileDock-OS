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
import { StatusBadge } from '../../components/StatusBadge.tsx';
import { StateView } from '../../components/StateView.tsx';
import {
  useResourceAction,
  type Resource,
  type PageProps,
} from '../resources/useResourceAction.ts';
import { useUnsavedChanges } from '../../navigation/useUnsavedChanges.ts';
export function AppsPage({ canMutate = true, connected = true }: PageProps) {
  const pagination = useResourcePage();
  const apps = useQuery({
    queryKey: ['apps', pagination.cursor],
    queryFn: () =>
      api.get<{ items: Resource[]; nextPageToken?: string | null }>(`/apps${pagination.query}`),
    refetchInterval: 10000,
  });
  const action = useResourceAction(canMutate, connected, ['apps']);
  const [editing, setEditing] = useState<Resource | null | undefined>(undefined);
  const [removing, setRemoving] = useState<Resource | null>(null);
  const [name, setName] = useState('');
  const [compose, setCompose] = useState('');
  const [environment, setEnvironment] = useState('{}');
  const [secrets, setSecrets] = useState('');
  const [dirty, setDirty] = useState(false);
  useUnsavedChanges(dirty);
  function open(row: Resource | null) {
    setEditing(row);
    setName(String(row?.spec.name ?? ''));
    setCompose(String(row?.spec.compose ?? ''));
    setEnvironment(JSON.stringify(row?.spec.environment ?? {}, null, 2));
    setSecrets(((row?.spec.secrets ?? []) as string[]).join('\n'));
    setDirty(false);
    action.setError('');
  }
  function close() {
    setEditing(undefined);
    setRemoving(null);
    setDirty(false);
    setCompose('');
  }
  return (
    <div className="stack resource-page">
      <PageHeader
        title="Docker apps"
        description="Run Compose applications with persistent data bound to managed folders."
        action={
          <Button disabled={action.disabled} onClick={() => open(null)}>
            Add application
          </Button>
        }
      />
      {action.error && editing === undefined && !removing ? (
        <p role="alert">{action.error}</p>
      ) : null}
      {action.operation ? (
        <a href={`#/settings/operations/${action.operation}`}>View app operation</a>
      ) : null}
      <Panel>
        <StateView
          phase={
            apps.isPending
              ? 'loading'
              : apps.error
                ? 'error'
                : apps.data?.items.length
                  ? 'ready'
                  : 'empty'
          }
          error={apps.error?.message}
          onRetry={() => void apps.refetch()}
          empty="No applications yet. Enable Docker access on a managed folder, then bind its /storage path in Compose."
        />
        {apps.data?.items.length ? (
          <DataTable
            label="Compose applications"
            headers={['Application', 'State', 'Data bindings', 'Actions']}
          >
            {apps.data.items.map((row) => (
              <tr key={row.id}>
                <td>
                  <strong>{String(row.spec.name)}</strong>
                </td>
                <td>
                  <StatusBadge state={String(row.status?.state ?? 'Pending')} />
                </td>
                <td>
                  {Array.isArray(row.status?.bindingWarnings) &&
                  row.status.bindingWarnings.length ? (
                    <ul>
                      {row.status.bindingWarnings.map((w, i) => (
                        <li key={i}>{String(w)}</li>
                      ))}
                    </ul>
                  ) : (
                    <span>Review the application’s managed folder bindings.</span>
                  )}
                </td>
                <td>
                  <RowActions
                    label={`Actions for ${row.spec.name}`}
                    actions={[
                      {
                        label: 'Edit Compose',
                        disabled: action.disabled,
                        onClick: () => open(row),
                      },
                      ...['start', 'stop', 'update'].map((a) => ({
                        label: a[0]!.toUpperCase() + a.slice(1),
                        disabled: action.disabled,
                        onClick: () => void action.act(() => api.post(`/apps/${row.id}/${a}`)),
                      })),
                      {
                        label: 'Remove',
                        danger: true,
                        disabled: action.disabled,
                        onClick: () => setRemoving(row),
                      },
                    ]}
                  />
                </td>
              </tr>
            ))}
          </DataTable>
        ) : null}
      </Panel>
      {pagination.controls(apps.data?.nextPageToken)}
      <Dialog
        open={editing !== undefined}
        title={editing ? 'Edit application' : 'Add application'}
        onClose={close}
        dirty={dirty}
        busy={action.busy}
        footer={
          <Button
            disabled={action.disabled || !name || !compose}
            busy={action.busy}
            onClick={() =>
              void action.act(() => {
                const values = {
                  name,
                  compose,
                  environment: JSON.parse(environment),
                  secrets: secrets
                    .split('\n')
                    .map((s) => s.trim())
                    .filter(Boolean),
                };
                return editing
                  ? api.patch(`/apps/${editing.id}`, values, String(editing.generation))
                  : api.post('/apps', values);
              }, close)
            }
          >
            Save application
          </Button>
        }
      >
        <Field
          label="Application name"
          value={name}
          readOnly={!!editing}
          onChange={(e) => {
            setName(e.target.value);
            setDirty(true);
          }}
        />
        <TextAreaField
          label="Compose YAML"
          rows={16}
          value={compose}
          onChange={(e) => {
            setCompose(e.target.value);
            setDirty(true);
          }}
          hint="Use /storage/<folder> bind sources with Docker access. Named volumes and external paths are outside FileDock protection."
        />
        <details>
          <summary>Environment and vault references</summary>
          <TextAreaField
            label="Environment (JSON)"
            rows={4}
            value={environment}
            onChange={(e) => {
              setEnvironment(e.target.value);
              setDirty(true);
            }}
            hint="String values only. Keep passwords in the encrypted vault."
          />
          <TextAreaField
            label="Secret references (one per line)"
            rows={3}
            value={secrets}
            onChange={(e) => {
              setSecrets(e.target.value);
              setDirty(true);
            }}
            hint="Use existing vault references. Compose secret names replace / with __."
          />
        </details>
        <p className="warning">
          An application can request broad host access. Review its Compose definition before running
          it.
        </p>
        {action.error ? <p role="alert">{action.error}</p> : null}
      </Dialog>
      <Dialog
        open={!!removing}
        title={`Remove ${removing?.spec.name ?? 'application'}?`}
        onClose={close}
        busy={action.busy}
        footer={
          <Button
            variant="danger"
            disabled={action.disabled}
            onClick={() =>
              removing &&
              void action.act(
                () => api.del(`/apps/${removing.id}`, String(removing.generation)),
                close,
              )
            }
          >
            Remove application
          </Button>
        }
      >
        <p>This stops its containers. Managed folder data and Docker volumes are kept.</p>
        {action.error ? <p role="alert">{action.error}</p> : null}
      </Dialog>
    </div>
  );
}
