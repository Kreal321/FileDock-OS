import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { api } from '../../api/client.ts';
type Admin = { id: string; username: string; enabled: boolean; lockedUntil: string | null };
export function AdminsPage({ onSignedOut }: { onSignedOut: () => void }) {
  const cache = useQueryClient();
  const admins = useQuery({ queryKey: ['admins'], queryFn: () => api.get<Admin[]>('/admins') });
  const me = useQuery({
    queryKey: ['me'],
    queryFn: () => api.get<{ id: string; username: string }>('/auth/me'),
  });
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  async function act(action: () => Promise<unknown>) {
    setError('');
    setBusy(true);
    try {
      await action();
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
      <h1>Administrators</h1>
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
        <label>
          Username
          <input name="username" required maxLength={64} autoComplete="off" />
        </label>
        <label>
          Password
          <input name="password" type="password" required autoComplete="new-password" />
        </label>
        <p>We recommend more than 8 characters.</p>
        <label>
          Re-enter password
          <input name="passwordConfirmation" type="password" required autoComplete="new-password" />
        </label>
        <button type="submit" className="primary" disabled={busy}>
          Create administrator
        </button>
      </form>
      {(admins.data ?? []).map((admin) => (
        <article className="panel stack" key={admin.id}>
          <h2>
            {admin.username}
            {admin.id === me.data?.id ? ' (you)' : ''}
          </h2>
          <p>
            {admin.enabled ? 'Enabled' : 'Disabled'}
            {admin.lockedUntil && new Date(admin.lockedUntil) > new Date()
              ? ` · Locked until ${new Date(admin.lockedUntil).toLocaleString()}`
              : ''}
          </p>
          <button
            className="ghost"
            disabled={busy}
            onClick={() =>
              void act(async () => {
                await api.patch(`/admins/${admin.id}`, { enabled: !admin.enabled });
                if (admin.id === me.data?.id && admin.enabled) onSignedOut();
              })
            }
          >
            {admin.enabled ? 'Disable' : 'Enable'} {admin.username}
          </button>
          <details>
            <summary>Remove {admin.username}</summary>
            <p>This removes the account. To retain it, disable it instead.</p>
            <button
              className="ghost"
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  await api.delete(`/admins/${admin.id}`);
                  if (admin.id === me.data?.id) onSignedOut();
                })
              }
            >
              Remove account
            </button>
          </details>
        </article>
      ))}
      <form className="panel stack" onSubmit={password}>
        <h2>Change your password</h2>
        <p>
          This signs out all your sessions. Resetting another account's password requires the local
          console.
        </p>
        <label>
          Current password
          <input type="password" name="currentPassword" required autoComplete="current-password" />
        </label>
        <label>
          New password
          <input type="password" name="password" required autoComplete="new-password" />
        </label>
        <label>
          Re-enter new password
          <input type="password" name="passwordConfirmation" required autoComplete="new-password" />
        </label>
        <button className="primary" disabled={busy || !me.data} type="submit">
          Change password
        </button>
      </form>
      <button
        className="ghost"
        disabled={busy}
        onClick={() =>
          void act(async () => {
            await api.post('/auth/sign-out-all');
            onSignedOut();
          })
        }
      >
        Sign out all sessions
      </button>
    </section>
  );
}
