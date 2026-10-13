import { Panel } from '../../components/Panel.tsx';
import { PageHeader } from '../../components/PageHeader.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
import { dataView } from '../../api/view-models.ts';
import { StateView } from '../../components/StateView.tsx';
import { Select } from '../../components/Select.tsx';
import { Field } from '../../components/Field.tsx';
import { Button } from '../../components/Button.tsx';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { api } from '../../api/client.ts';

type Token = {
  id: string;
  name: string;
  scope: string;
  expiresAt: string | null;
  revokedAt: string | null;
  lastUsedAt: string | null;
};

export function TokensPage({
  canMutate = true,
  connected = true,
}: {
  canMutate?: boolean;
  connected?: boolean;
}) {
  const cache = useQueryClient();
  const tokens = useQuery({ queryKey: ['tokens'], queryFn: () => api.get<Token[]>('/tokens') });
  const [secret, setSecret] = useState('');
  const active = useRef(true);
  useEffect(() => {
    active.current = true;
    const clear = () => {
      active.current = false;
      setSecret('');
    };
    window.addEventListener('filedock:session-expired', clear);
    return () => {
      active.current = false;
      window.removeEventListener('filedock:session-expired', clear);
    };
  }, []);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [revoking, setRevoking] = useState<Token | null>(null);
  async function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canMutate || busy) return;
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
      if (active.current) setSecret(result.token);
      element.reset();
      await cache.invalidateQueries({ queryKey: ['tokens'] });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not create token.');
    } finally {
      setBusy(false);
    }
  }
  async function revoke(id: string) {
    if (!canMutate || busy) return;
    setError('');
    setBusy(true);
    try {
      await api.delete(`/tokens/${encodeURIComponent(id)}`);
      setRevoking(null);
      await cache.invalidateQueries({ queryKey: ['tokens'] });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not revoke token.');
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="stack">
      <PageHeader title="API tokens" description="Connect applications with scoped access." />
      <StateView
        {...dataView(tokens.data, tokens.isPending, tokens.error, tokens.dataUpdatedAt, !connected)}
        onRetry={() => void tokens.refetch()}
        empty="No records returned."
      />
      <p>
        Read-only tokens can view status and lists. Full tokens can change resources. Account, token
        and network administration require a signed-in session.
      </p>
      <form className="panel stack" onSubmit={create}>
        <Field label="Token name" name="name" required maxLength={128} />
        <Select label="Scope" name="scope" defaultValue="ReadOnly">
          <option value="ReadOnly">Read-only</option>
          <option value="Full">Full</option>
        </Select>
        <Field label="Expires at (optional)" name="expiresAt" type="datetime-local" />
        <Button variant="primary" disabled={!canMutate} busy={busy} type="submit">
          Create token
        </Button>
      </form>
      {secret ? (
        <div className="panel stack">
          <p>Save this token now. It will not be shown again.</p>
          <code className="recovery-key" data-secret>
            {secret}
          </code>
          <Button variant="secondary" onClick={() => setSecret('')}>
            I saved the token
          </Button>
        </div>
      ) : null}
      {error || tokens.error ? (
        <p role="alert" className="error">
          {error || tokens.error?.message}
        </p>
      ) : null}
      {tokens.isPending ? <p role="status">Loading tokens…</p> : null}
      {tokens.data?.length === 0 ? (
        <p className="muted">No API tokens yet. Create one to connect an application.</p>
      ) : null}
      {tokens.data?.length ? (
        <Panel className="stack">
          <h2>Issued tokens</h2>
          <DataTable
            label="API tokens"
            headers={['Name', 'Scope', 'Status', 'Expires', 'Last used', 'Actions']}
          >
            {tokens.data.map((token) => (
              <tr key={token.id}>
                <td>{token.name}</td>
                <td>{token.scope}</td>
                <td>
                  <StatusBadge
                    state={
                      token.revokedAt
                        ? 'Revoked'
                        : token.expiresAt && new Date(token.expiresAt) <= new Date()
                          ? 'Expired'
                          : 'Active'
                    }
                  />
                </td>
                <td>{token.expiresAt ? new Date(token.expiresAt).toLocaleString() : 'Never'}</td>
                <td>{token.lastUsedAt ? new Date(token.lastUsedAt).toLocaleString() : 'Never'}</td>
                <td>
                  {!token.revokedAt ? (
                    <Button
                      variant="danger"
                      busy={busy}
                      disabled={!canMutate}
                      onClick={() => {
                        setError('');
                        setRevoking(token);
                      }}
                    >
                      Revoke {token.name}
                    </Button>
                  ) : null}
                </td>
              </tr>
            ))}
          </DataTable>
        </Panel>
      ) : null}
      <Dialog
        open={!!revoking}
        title={`Revoke ${revoking?.name ?? 'token'}?`}
        busy={busy}
        onClose={() => setRevoking(null)}
      >
        <p>Applications using this token will lose access.</p>
        {error ? (
          <p className="error" role="alert">
            {error}
          </p>
        ) : null}
        <Button
          variant="danger"
          busy={busy}
          disabled={!canMutate}
          onClick={() => {
            if (revoking) void revoke(revoking.id);
          }}
        >
          Revoke token
        </Button>
      </Dialog>
    </section>
  );
}
