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
type Folder = { id: string; identity: { name: string; fileDockPath: string }; access: string[] };
export function SharesPage({ canMutate = true, connected = true }: PageProps) {
  const pagination = useResourcePage();
  const users = useQuery({
    queryKey: ['share-users', pagination.cursor],
    queryFn: () =>
      api.get<{ items: Resource[]; nextPageToken?: string | null }>(
        `/share-users${pagination.query}`,
      ),
  });
  const folders = useQuery({
    queryKey: ['share-folders'],
    queryFn: () => api.get<{ items: Folder[] }>('/folders?pageSize=500'),
  });
  const action = useResourceAction(canMutate, connected, ['share-users', 'share-exports']);
  const [editing, setEditing] = useState<Resource | null | undefined>(undefined);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [enabled, setEnabled] = useState(true);
  const [dirty, setDirty] = useState(false);
  const [removing, setRemoving] = useState<Resource | null>(null);
  const [folder, setFolder] = useState('');
  const [protocol, setProtocol] = useState('Smb');
  const [rules, setRules] = useState<string | null>(null);
  const exports = useQuery({
    queryKey: ['share-exports', folder],
    queryFn: () =>
      api.get<{ items: Resource[]; nextPageToken?: string | null }>(`/folders/${folder}/exports`),
    enabled: !!folder,
  });
  useUnsavedChanges(dirty);
  function close() {
    setEditing(undefined);
    setRemoving(null);
    setRules(null);
    setPassword('');
    setDirty(false);
    action.setError('');
  }
  function open(row: Resource | null) {
    setEditing(row);
    setUsername(String(row?.spec.username ?? ''));
    setPassword('');
    setEnabled(row?.spec.enabled !== false);
    setDirty(false);
    action.setError('');
  }
  return (
    <div className="stack resource-page">
      <PageHeader
        title="Shares"
        description="Share users access files over SMB or WebDAV. They cannot sign in to appliance management."
        action={
          <Button disabled={action.disabled} onClick={() => open(null)}>
            Add share user
          </Button>
        }
      />
      {action.error && editing === undefined && !rules && !removing ? (
        <p role="alert">{action.error}</p>
      ) : null}
      <Panel className="stack">
        <h2>Share users</h2>
        <StateView
          phase={
            users.isPending
              ? 'loading'
              : users.error
                ? 'error'
                : users.data?.items.length
                  ? 'ready'
                  : 'empty'
          }
          error={users.error?.message}
          onRetry={() => void users.refetch()}
          empty="No share users yet. Access remains denied until you add explicit export rules."
        />
        {users.data?.items.length ? (
          <DataTable label="Share accounts" headers={['User', 'Access', 'Status', 'Actions']}>
            {users.data.items.map((r) => (
              <tr key={r.id}>
                <td>
                  <strong>{String(r.spec.username)}</strong>
                </td>
                <td>{r.spec.enabled === false ? 'Disabled' : 'Enabled'}</td>
                <td>
                  <StatusBadge state={String(r.status?.state ?? 'Pending')} />
                  {r.status?.detail ? (
                    <div className="caption muted">{String(r.status.detail)}</div>
                  ) : null}
                </td>
                <td>
                  <RowActions
                    label={`Actions for ${r.spec.username}`}
                    actions={[
                      { label: 'Edit account', disabled: action.disabled, onClick: () => open(r) },
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
      <Panel className="stack">
        <h2>Folder exports</h2>
        <label>
          Managed folder
          <select
            value={folder}
            onChange={(e) => {
              setFolder(e.target.value);
              setRules(null);
            }}
          >
            <option value="">Choose a folder</option>
            {folders.data?.items.map((f) => (
              <option key={f.id} value={f.id}>
                {f.identity.name} · {f.identity.fileDockPath}
              </option>
            ))}
          </select>
        </label>
        {folders.error ? <p role="alert">{folders.error.message}</p> : null}
        {folder ? (
          <>
            <StateView
              phase={
                exports.isPending
                  ? 'loading'
                  : exports.error
                    ? 'error'
                    : exports.data?.items.length
                      ? 'ready'
                      : 'empty'
              }
              error={exports.error?.message}
              onRetry={() => void exports.refetch()}
              empty="No exports selected for this folder. Enable access in its Managed Folder settings first."
            />
            {exports.data?.items.length ? (
              <DataTable
                label="Folder export rules"
                headers={['Protocol', 'State', 'Endpoints', 'Actions']}
              >
                {exports.data.items.map((r) => (
                  <tr key={r.id}>
                    <td>{String(r.spec.protocol)}</td>
                    <td>{String(r.status?.state ?? 'Pending')}</td>
                    <td>
                      {Array.isArray(r.status?.endpoints)
                        ? r.status.endpoints.map((e: { port: number; access: string }) => (
                            <div key={e.port}>
                              HTTP port {e.port} · {e.access}
                            </div>
                          ))
                        : r.spec.protocol === 'Smb'
                          ? `\\\\${window.location.hostname}\\${r.id}`
                          : 'NFSv4 · sec=sys'}
                    </td>
                    <td>
                      <Button
                        size="compact"
                        variant="secondary"
                        disabled={action.disabled}
                        onClick={() => {
                          setProtocol(String(r.spec.protocol));
                          setRules(JSON.stringify(r.spec.rules ?? [], null, 2));
                          setDirty(false);
                          action.setError('');
                        }}
                      >
                        Edit rules
                      </Button>
                    </td>
                  </tr>
                ))}
              </DataTable>
            ) : null}
          </>
        ) : null}
        <p className="muted">
          NFS uses trusted client IP/CIDR rules and sec=sys. WebDAV uses HTTP; mixed read/write
          rules have separate authenticated endpoints.
        </p>
      </Panel>
      {pagination.controls(users.data?.nextPageToken)}
      <Dialog
        open={editing !== undefined}
        title={editing ? 'Edit share user' : 'Add share user'}
        onClose={close}
        dirty={dirty}
        busy={action.busy}
        footer={
          <Button
            disabled={action.disabled || !username || (!editing && !password)}
            busy={action.busy}
            onClick={() =>
              void action.act(
                () =>
                  editing
                    ? api.patch(
                        `/share-users/${editing.id}`,
                        { username, password: password || undefined, enabled },
                        String(editing.generation),
                      )
                    : api.post('/share-users', { username, password, enabled }),
                close,
              )
            }
          >
            Save share user
          </Button>
        }
      >
        <Field
          label="Username"
          value={username}
          readOnly={!!editing}
          onChange={(e) => {
            setUsername(e.target.value);
            setDirty(true);
          }}
        />
        <Field
          label={editing ? 'New password (leave empty to keep)' : 'Password'}
          type="password"
          autoComplete="new-password"
          value={password}
          onChange={(e) => {
            setPassword(e.target.value);
            setDirty(true);
          }}
        />
        <label>
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => {
              setEnabled(e.target.checked);
              setDirty(true);
            }}
          />{' '}
          Enabled
        </label>
        {action.error ? <p role="alert">{action.error}</p> : null}
      </Dialog>
      <Dialog
        open={rules !== null}
        title={`${protocol} access rules`}
        onClose={close}
        dirty={dirty}
        busy={action.busy}
        footer={
          <Button
            disabled={action.disabled}
            busy={action.busy}
            onClick={() =>
              void action.act(() => {
                const old = exports.data?.items.find((r) => r.spec.protocol === protocol);
                return api.put(
                  `/folders/${folder}/exports/${protocol.toLowerCase()}`,
                  { rules: JSON.parse(rules ?? '[]') },
                  old ? String(old.generation) : undefined,
                );
              }, close)
            }
          >
            Save access rules
          </Button>
        }
      >
        <TextAreaField
          label="Access rules (JSON)"
          rows={12}
          value={rules ?? ''}
          onChange={(e) => {
            setRules(e.target.value);
            setDirty(true);
          }}
          hint={
            protocol === 'Nfs'
              ? 'Example: [{"client":"192.168.68.0/24","access":"rw"}]'
              : 'Example: [{"principal":"user:alice","access":"read-write"}]'
          }
        />
        <p>No rules means no access. Read-only and read-write permissions remain distinct.</p>
        {action.error ? <p role="alert">{action.error}</p> : null}
      </Dialog>
      <Dialog
        open={!!removing}
        title={`Remove ${removing?.spec.username ?? 'share user'}?`}
        onClose={close}
        busy={action.busy}
        footer={
          <Button
            variant="danger"
            disabled={action.disabled}
            onClick={() =>
              removing &&
              void action.act(
                () => api.del(`/share-users/${removing.id}`, String(removing.generation)),
                close,
              )
            }
          >
            Remove share user
          </Button>
        }
      >
        <p>Remove this user from export rules first. Its files are kept.</p>
        {action.error ? <p role="alert">{action.error}</p> : null}
      </Dialog>
    </div>
  );
}
