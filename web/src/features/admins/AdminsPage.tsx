import { Panel } from '../../components/Panel.tsx';
import { PageHeader } from '../../components/PageHeader.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
import { dataView } from '../../api/view-models.ts';
import { StateView } from '../../components/StateView.tsx';
import { Field } from '../../components/Field.tsx';
import { Button } from '../../components/Button.tsx';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { api } from '../../api/client.ts';
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
  const [removing, setRemoving] = useState<Admin | null>(null);
  async function act(action: () => Promise<unknown>) {
    if (!canMutate || busy) return;
    setError('');
    setBusy(true);
    try {
      await action();
      setRemoving(null);
      await cache.invalidateQueries({ queryKey: ['admins'] });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not save administrator.');
    } finally {
      setBusy(false);
    }
  }
  async function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const element = event.currentTarget;
    const form = new FormData(element);
    if (form.get('password') !== form.get('passwordConfirmation')) {
      setError('Passwords do not match.');
      return;
    }
    await act(async () => {
      await api.post('/admins', Object.fromEntries(form));
      element.reset();
    });
  }
  async function password(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!me.data) return;
    const form = new FormData(event.currentTarget);
    if (form.get('password') !== form.get('passwordConfirmation')) {
      setError('Passwords do not match.');
      return;
    }
    await act(async () => {
      await api.post(`/admins/${me.data!.id}/password`, Object.fromEntries(form));
      onSignedOut();
    });
  }
  return (
    <section className="stack">
      <PageHeader
        title="Administrators"
        description="Manage appliance accounts and your password."
      />
      <StateView
        {...dataView(admins.data, admins.isPending, admins.error, admins.dataUpdatedAt, !connected)}
        onRetry={() => void admins.refetch()}
        empty="No records returned."
      />
      <p>
        Every administrator has full access. The last enabled account cannot be disabled or removed.
      </p>
      {error || admins.error || me.error ? (
        <p className="error" role="alert">
          {error || admins.error?.message || me.error?.message}
        </p>
      ) : null}
      <form className="panel stack" onSubmit={create}>
        <h2>Create administrator</h2>
        <Field label="Username" name="username" required maxLength={64} autoComplete="off" />
        <Field
          label="Password"
          name="password"
          type="password"
          required
          autoComplete="new-password"
        />
        <p>We recommend more than 8 characters.</p>
        <Field
          label="Re-enter password"
          name="passwordConfirmation"
          type="password"
          required
          autoComplete="new-password"
        />
        <Button type="submit" variant="primary" disabled={!canMutate} busy={busy}>
          Create administrator
        </Button>
      </form>
      {admins.isPending ? <p role="status">Loading administrators…</p> : null}
      {admins.data?.length === 0 ? <p>No administrators returned.</p> : null}
      {admins.data?.length ? (
        <Panel className="stack">
          <h2>Accounts</h2>
          <DataTable label="Administrators" headers={['Username', 'Status', 'Actions']}>
            {admins.data.map((admin) => {
              const lastEnabled =
                admin.enabled && admins.data.filter((a) => a.enabled).length === 1;
              return (
                <tr key={admin.id}>
                  <td>
                    {admin.username}
                    {admin.id === me.data?.id ? ' (you)' : ''}
                  </td>
                  <td>
                    <StatusBadge state={admin.enabled ? 'Enabled' : 'Disabled'} />
                    {admin.lockedUntil && new Date(admin.lockedUntil) > new Date() ? (
                      <p className="caption">
                        Locked until {new Date(admin.lockedUntil).toLocaleString()}
                      </p>
                    ) : null}
                  </td>
                  <td>
                    <div className="actions">
                      <Button
                        variant="secondary"
                        busy={busy}
                        disabled={!canMutate || lastEnabled}
                        onClick={() =>
                          void act(async () => {
                            await api.patch(`/admins/${encodeURIComponent(admin.id)}`, {
                              enabled: !admin.enabled,
                            });
                            if (admin.id === me.data?.id && admin.enabled) onSignedOut();
                          })
                        }
                      >
                        {admin.enabled ? 'Disable' : 'Enable'} {admin.username}
                      </Button>
                      <Button
                        variant="danger"
                        busy={busy}
                        disabled={!canMutate || lastEnabled}
                        onClick={() => {
                          setError('');
                          setRemoving(admin);
                        }}
                      >
                        Remove {admin.username}
                      </Button>
                    </div>
                    {lastEnabled ? (
                      <p className="caption muted">Last enabled administrator</p>
                    ) : null}
                  </td>
                </tr>
              );
            })}
          </DataTable>
        </Panel>
      ) : null}
      <Dialog
        open={!!removing}
        title={`Remove ${removing?.username ?? 'administrator'}?`}
        onClose={() => setRemoving(null)}
        busy={busy}
      >
        <p>
          This permanently removes the account and ends its sessions. To retain it, disable it
          instead.
        </p>
        {error ? (
          <p className="error" role="alert">
            {error}
          </p>
        ) : null}
        <Button
          variant="danger"
          disabled={!canMutate}
          busy={busy}
          onClick={() => {
            if (removing)
              void act(async () => {
                await api.delete(`/admins/${encodeURIComponent(removing.id)}`);
                if (removing.id === me.data?.id) onSignedOut();
              });
          }}
        >
          Remove account
        </Button>
      </Dialog>
      <form className="panel stack" onSubmit={password}>
        <h2>Change your password</h2>
        <p>
          This signs out all your sessions. Resetting another account's password requires the local
          console.
        </p>
        <Field
          label="Current password"
          type="password"
          name="currentPassword"
          required
          autoComplete="current-password"
        />
        <Field
          label="New password"
          type="password"
          name="password"
          required
          autoComplete="new-password"
        />
        <Field
          label="Re-enter new password"
          type="password"
          name="passwordConfirmation"
          required
          autoComplete="new-password"
        />
        <Button variant="primary" disabled={!canMutate || !me.data} busy={busy} type="submit">
          Change password
        </Button>
      </form>
      <Button
        variant="secondary"
        disabled={!canMutate}
        busy={busy}
        onClick={() =>
          void act(async () => {
            await api.post('/auth/sign-out-all');
            onSignedOut();
          })
        }
      >
        Sign out all sessions
      </Button>
    </section>
  );
}
