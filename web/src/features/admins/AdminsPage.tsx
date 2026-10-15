import { useEffect, useId, useState, type FormEvent } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api } from '../../api/client.ts';
import { dataView } from '../../api/view-models.ts';
import { useUnsavedChanges } from '../../navigation/useUnsavedChanges.ts';
import { Panel } from '../../components/Panel.tsx';
import { PageHeader } from '../../components/PageHeader.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { TableToolbar } from '../../components/TableToolbar.tsx';
import { SettingsRow } from '../../components/SettingsRow.tsx';
import { RowActions } from '../../components/RowActions.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
import { StateView } from '../../components/StateView.tsx';
import { Field } from '../../components/Field.tsx';
import { Button } from '../../components/Button.tsx';
type Admin = { id: string; username: string; enabled: boolean; lockedUntil: string | null };
export function AdminsPage({
  onSignedOut,
  canMutate = true,
  connected = true,
}: {
  onSignedOut: () => void;
  canMutate?: boolean;
  connected?: boolean;
}) {
  const cache = useQueryClient();
  const admins = useQuery({ queryKey: ['admins'], queryFn: () => api.get<Admin[]>('/admins') });
  const me = useQuery({
    queryKey: ['me'],
    queryFn: () => api.get<{ id: string; username: string }>('/auth/me'),
  });
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState<'create' | 'password' | 'sessions' | null>(null);
  const [removing, setRemoving] = useState<Admin | null>(null);
  const [dirty, setDirty] = useState(false);
  const formId = useId();
  useUnsavedChanges(dirty);
  function close() {
    setEditing(null);
    setRemoving(null);
    setDirty(false);
    setError('');
  }
  useEffect(() => {
    const clear = () => {
      setEditing(null);
      setRemoving(null);
      setDirty(false);
      setError('');
    };
    window.addEventListener('filedock:session-expired', clear);
    return () => window.removeEventListener('filedock:session-expired', clear);
  }, []);
  function open(kind: 'create' | 'password' | 'sessions') {
    setError('');
    setDirty(false);
    setEditing(kind);
  }
  async function act(action: () => Promise<unknown>) {
    if (!canMutate || busy) return;
    setError('');
    setBusy(true);
    try {
      await action();
      close();
      await cache.invalidateQueries({ queryKey: ['admins'] });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not save administrator.');
    } finally {
      setBusy(false);
    }
  }
  function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!canMutate || busy) return;
    const data = new FormData(e.currentTarget);
    if (data.get('password') !== data.get('passwordConfirmation')) {
      setError('Passwords do not match.');
      return;
    }
    void act(async () => {
      if (editing === 'create') await api.post('/admins', Object.fromEntries(data));
      else if (me.data) {
        await api.post(
          `/admins/${encodeURIComponent(me.data.id)}/password`,
          Object.fromEntries(data),
        );
        onSignedOut();
      }
    });
  }
  const title = removing
    ? `Remove ${removing.username}?`
    : editing === 'create'
      ? 'Add administrator'
      : editing === 'password'
        ? 'Change password'
        : 'Sign out all sessions?';
  return (
    <section className="stack">
      <PageHeader
        title="Administrators"
        description="Manage appliance accounts and your password."
        action={
          <Button
            aria-label="Add administrator"
            disabled={!canMutate}
            onClick={() => open('create')}
          >
            + Add administrator
          </Button>
        }
      />
      <StateView
        {...dataView(admins.data, admins.isPending, admins.error, admins.dataUpdatedAt, !connected)}
        onRetry={() => void admins.refetch()}
        empty="No administrator accounts returned."
      />
      {!editing && !removing && (error || me.error) ? (
        <p className="error" role="alert">
          {error || me.error?.message}
        </p>
      ) : null}
      {admins.data?.length ? (
        <Panel className="stack">
          <TableToolbar title="Accounts" />
          <DataTable
            label="Administrators"
            headers={[
              { id: 'username', label: 'Username' },
              { id: 'access', label: 'Access' },
              { id: 'status', label: 'Status' },
              { id: 'actions', label: 'Actions', align: 'actions' },
            ]}
          >
            {admins.data.map((admin) => {
              const last = admin.enabled && admins.data.filter((a) => a.enabled).length === 1;
              return (
                <tr key={admin.id}>
                  <td>
                    <span className="identity-name">{admin.username}</span>
                    {admin.id === me.data?.id ? <span className="account-tag">You</span> : null}
                  </td>
                  <td>Full administrator</td>
                  <td>
                    <StatusBadge state={admin.enabled ? 'Enabled' : 'Disabled'} />
                    {last ? (
                      <span className="identity-detail">Last enabled administrator</span>
                    ) : null}
                    {admin.lockedUntil && new Date(admin.lockedUntil) > new Date() ? (
                      <span className="identity-detail">
                        Locked until {new Date(admin.lockedUntil).toLocaleString()}
                      </span>
                    ) : null}
                  </td>
                  <td>
                    <RowActions
                      label={`Actions for ${admin.username}`}
                      actions={[
                        {
                          label: `${admin.enabled ? 'Disable' : 'Enable'} ${admin.username}`,
                          disabled: !canMutate || busy || last,
                          onClick: () =>
                            void act(async () => {
                              await api.patch(`/admins/${encodeURIComponent(admin.id)}`, {
                                enabled: !admin.enabled,
                              });
                              if (admin.id === me.data?.id && admin.enabled) onSignedOut();
                            }),
                        },
                        {
                          label: `Remove ${admin.username}`,
                          danger: true,
                          disabled: !canMutate || busy || last,
                          onClick: () => {
                            setError('');
                            setRemoving(admin);
                          },
                        },
                      ]}
                    />
                  </td>
                </tr>
              );
            })}
          </DataTable>
          <p className="caption muted">At least one administrator account must remain enabled.</p>
        </Panel>
      ) : null}
      <Panel>
        <SettingsRow
          title="Your password"
          description="Update your sign-in password. Resetting another account requires the local console."
          action={
            <Button
              variant="secondary"
              size="compact"
              disabled={!canMutate || !me.data}
              onClick={() => open('password')}
            >
              Change password
            </Button>
          }
        />
        <SettingsRow
          title="Your sessions"
          description="End all of your sessions on this appliance."
          action={
            <Button
              variant="quiet"
              size="compact"
              disabled={!canMutate}
              onClick={() => open('sessions')}
            >
              Sign out all sessions
            </Button>
          }
        />
      </Panel>
      <Dialog
        open={!!editing || !!removing}
        title={title}
        description={
          editing === 'password'
            ? 'This signs out all your sessions.'
            : removing
              ? 'This permanently removes the account and ends its sessions.'
              : editing === 'sessions'
                ? 'You will need to sign in again on every device.'
                : undefined
        }
        onClose={close}
        dirty={dirty}
        busy={busy}
        footer={
          editing === 'create' || editing === 'password' ? (
            <Button
              form={formId}
              type="submit"
              busy={busy}
              disabled={!canMutate || (editing === 'password' && !me.data)}
            >
              {editing === 'create' ? 'Create administrator' : 'Update password'}
            </Button>
          ) : (
            <Button
              variant="danger"
              busy={busy}
              disabled={!canMutate}
              onClick={() =>
                void act(async () => {
                  if (removing) {
                    await api.delete(`/admins/${encodeURIComponent(removing.id)}`);
                    if (removing.id === me.data?.id) onSignedOut();
                  } else {
                    await api.post('/auth/sign-out-all');
                    onSignedOut();
                  }
                })
              }
            >
              {removing ? 'Remove account' : 'Sign out all sessions'}
            </Button>
          )
        }
      >
        {error ? (
          <p className="error" role="alert">
            {error}
          </p>
        ) : null}
        {editing === 'create' || editing === 'password' ? (
          <form id={formId} className="stack" onSubmit={submit} onChange={() => setDirty(true)}>
            {editing === 'create' ? (
              <Field label="Username" name="username" required maxLength={64} autoComplete="off" />
            ) : (
              <Field
                label="Current password"
                name="currentPassword"
                type="password"
                required
                autoComplete="current-password"
              />
            )}
            <Field
              label={editing === 'create' ? 'Password' : 'New password'}
              name="password"
              type="password"
              required
              autoComplete="new-password"
              hint="We recommend more than 8 characters."
            />
            <Field
              label={editing === 'create' ? 'Re-enter password' : 'Re-enter new password'}
              name="passwordConfirmation"
              type="password"
              required
              autoComplete="new-password"
            />
          </form>
        ) : null}
      </Dialog>
    </section>
  );
}
