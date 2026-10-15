import { useEffect, useId, useRef, useState, type FormEvent } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api } from '../../api/client.ts';
import { dataView } from '../../api/view-models.ts';
import { useUnsavedChanges } from '../../navigation/useUnsavedChanges.ts';
import { Panel } from '../../components/Panel.tsx';
import { PageHeader } from '../../components/PageHeader.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { TableToolbar } from '../../components/TableToolbar.tsx';
import { RowActions } from '../../components/RowActions.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
import { StateView } from '../../components/StateView.tsx';
import { Field } from '../../components/Field.tsx';
import { Select } from '../../components/Select.tsx';
import { Button } from '../../components/Button.tsx';
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
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [creating, setCreating] = useState(false);
  const [revoking, setRevoking] = useState<Token | null>(null);
  const [dirty, setDirty] = useState(false);
  const formId = useId();
  const active = useRef(true);
  useUnsavedChanges(dirty);
  useEffect(() => {
    active.current = true;
    const clear = () => {
      active.current = false;
      setSecret('');
      setCreating(false);
      setRevoking(null);
      setDirty(false);
    };
    window.addEventListener('filedock:session-expired', clear);
    return () => {
      active.current = false;
      window.removeEventListener('filedock:session-expired', clear);
    };
  }, []);
  function close() {
    setSecret('');
    setCreating(false);
    setRevoking(null);
    setDirty(false);
    setError('');
  }
  async function create(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!canMutate || busy) return;
    const data = new FormData(e.currentTarget);
    setError('');
    setBusy(true);
    try {
      const expiry = String(data.get('expiresAt') ?? '');
      const result = await api.post<{ token: string }>('/tokens', {
        name: data.get('name'),
        scope: data.get('scope'),
        expiresAt: expiry ? new Date(expiry).toISOString() : null,
      });
      if (active.current) {
        setDirty(false);
        setCreating(false);
        setSecret(result.token);
      }
      await cache.invalidateQueries({ queryKey: ['tokens'] });
    } catch (e) {
      if (active.current) setError(e instanceof Error ? e.message : 'Could not create token.');
    } finally {
      setBusy(false);
    }
  }
  async function revoke() {
    if (!canMutate || busy || !revoking) return;
    setError('');
    setBusy(true);
    try {
      await api.delete(`/tokens/${encodeURIComponent(revoking.id)}`);
      close();
      await cache.invalidateQueries({ queryKey: ['tokens'] });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not revoke token.');
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="stack">
      <PageHeader
        title="API tokens"
        description="Connect applications with scoped access."
        action={
          <Button
            aria-label="Create token"
            disabled={!canMutate}
            onClick={() => {
              close();
              setCreating(true);
            }}
          >
            + Create token
          </Button>
        }
      />
      <StateView
        {...dataView(tokens.data, tokens.isPending, tokens.error, tokens.dataUpdatedAt, !connected)}
        onRetry={() => void tokens.refetch()}
        empty="No API tokens yet. Create one to connect an application."
      />
      <p className="muted">
        Read-only tokens can inspect resources. Full tokens can change resources. Account, token and
        network administration require a signed-in session.
      </p>
      {!creating && !revoking && !secret && error ? (
        <p className="error" role="alert">
          {error}
        </p>
      ) : null}
      {tokens.data?.length ? (
        <Panel className="stack">
          <TableToolbar title="Issued tokens" />
          <DataTable
            label="API tokens"
            headers={[
              'Name',
              'Scope',
              'Status',
              'Expires',
              'Last used',
              { id: 'actions', label: 'Actions', align: 'actions' },
            ]}
          >
            {tokens.data.map((token) => (
              <tr key={token.id}>
                <td className="identity-name">{token.name}</td>
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
                    <RowActions
                      label={`Actions for ${token.name}`}
                      actions={[
                        {
                          label: `Revoke ${token.name}`,
                          disabled: !canMutate || busy,
                          danger: true,
                          onClick: () => {
                            setError('');
                            setRevoking(token);
                          },
                        },
                      ]}
                    />
                  ) : null}
                </td>
              </tr>
            ))}
          </DataTable>
        </Panel>
      ) : null}
      <Dialog
        open={creating}
        title="Create token"
        onClose={close}
        dirty={dirty}
        busy={busy}
        footer={
          <Button form={formId} type="submit" busy={busy} disabled={!canMutate}>
            Create token
          </Button>
        }
      >
        {error ? (
          <p className="error" role="alert">
            {error}
          </p>
        ) : null}
        <form id={formId} className="stack" onSubmit={create} onChange={() => setDirty(true)}>
          <Field label="Token name" name="name" required maxLength={128} />
          <Select label="Scope" name="scope" defaultValue="ReadOnly">
            <option value="ReadOnly">Read-only</option>
            <option value="Full">Full</option>
          </Select>
          <Field label="Expires at (optional)" name="expiresAt" type="datetime-local" />
        </form>
      </Dialog>
      <Dialog
        open={!!secret}
        title="Save your token"
        description="Save this token now. It will not be shown again."
        onClose={close}
        footer={<Button onClick={close}>I saved the token</Button>}
      >
        <code className="recovery-key" data-secret>
          {secret}
        </code>
      </Dialog>
      <Dialog
        open={!!revoking}
        title={`Revoke ${revoking?.name ?? 'token'}?`}
        description="Applications using this token will lose access."
        onClose={close}
        busy={busy}
        footer={
          <Button variant="danger" busy={busy} disabled={!canMutate} onClick={() => void revoke()}>
            Revoke token
          </Button>
        }
      >
        {error ? (
          <p className="error" role="alert">
            {error}
          </p>
        ) : null}
      </Dialog>
    </section>
  );
}
