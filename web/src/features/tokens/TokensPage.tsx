import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { api } from '../../api/client.ts';

type Token = {
  id: string;
  name: string;
  scope: string;
  expiresAt: string | null;
  revokedAt: string | null;
  lastUsedAt: string | null;
};

export function TokensPage() {
  const cache = useQueryClient();
  const tokens = useQuery({ queryKey: ['tokens'], queryFn: () => api.get<Token[]>('/tokens') });
  const [secret, setSecret] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  async function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const element = event.currentTarget;
    const form = new FormData(element);
    setError('');
    setBusy(true);
    setSecret('');
    try {
      const expiry = String(form.get('expiresAt') ?? '');
      const result = await api.post<{ token: string }>('/tokens', {
        name: form.get('name'),
        scope: form.get('scope'),
        expiresAt: expiry ? new Date(expiry).toISOString() : null,
      });
      setSecret(result.token);
      element.reset();
      await cache.invalidateQueries({ queryKey: ['tokens'] });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not create token.');
    } finally {
      setBusy(false);
    }
  }
  async function revoke(id: string) {
    setError('');
    setBusy(true);
    try {
      await api.delete(`/tokens/${id}`);
      await cache.invalidateQueries({ queryKey: ['tokens'] });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not revoke token.');
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="stack">
      <h1>API tokens</h1>
      <p>
        Read-only tokens can view status and lists. Full tokens can change resources. Account, token
        and network administration require a signed-in session.
      </p>
      <form className="panel stack" onSubmit={create}>
        <label>
          Token name
          <input name="name" required maxLength={128} />
        </label>
        <label>
          Scope
          <select name="scope" defaultValue="ReadOnly">
            <option value="ReadOnly">Read-only</option>
            <option value="Full">Full</option>
          </select>
        </label>
        <label>
          Expires at (optional)
          <input name="expiresAt" type="datetime-local" />
        </label>
        <button className="primary" disabled={busy} type="submit">
          Create token
        </button>
      </form>
      {secret ? (
        <div className="panel stack">
          <p>Save this token now. It will not be shown again.</p>
          <code className="recovery-key">{secret}</code>
          <button className="ghost" onClick={() => setSecret('')}>
            I saved the token
          </button>
        </div>
      ) : null}
      {error || tokens.error ? (
        <p role="alert" className="error">
          {error || tokens.error?.message}
        </p>
      ) : null}
      {(tokens.data ?? []).map((token) => (
        <article className="panel stack" key={token.id}>
          <h2>{token.name}</h2>
          <p>
            {token.scope} · {token.revokedAt ? 'Revoked' : 'Active'} · Expires:{' '}
            {token.expiresAt ? new Date(token.expiresAt).toLocaleString() : 'Never'}
          </p>
          <p>
            Last used: {token.lastUsedAt ? new Date(token.lastUsedAt).toLocaleString() : 'Never'}
          </p>
          {!token.revokedAt ? (
            <button className="ghost" disabled={busy} onClick={() => void revoke(token.id)}>
              Revoke {token.name}
            </button>
          ) : null}
        </article>
      ))}
    </section>
  );
}
