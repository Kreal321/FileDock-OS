import { useEffect, useRef, useState } from 'react';
import { api } from '../../api/client.ts';
import { useUnsavedChanges } from '../../navigation/useUnsavedChanges.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Field } from '../../components/Field.tsx';
import { Button } from '../../components/Button.tsx';
import { saveDestinationDraft, takeDestinationDraft } from './destinationDraft.ts';
export function StateProtectionEditorPage({ canMutate = false }: { canMutate?: boolean }) {
  const [busy, setBusy] = useState(false),
    [dirty, setDirty] = useState(false),
    [error, setError] = useState('');
  const active = useRef(true);
  useUnsavedChanges(dirty);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  return (
    <section className="stack">
      <PageHeader
        title="Create a backup connection"
        description="Connect object storage for encrypted appliance configuration. This does not back up folder data."
      />
      <form
        className="panel stack"
        onChange={() => setDirty(true)}
        onSubmit={async (e) => {
          e.preventDefault();
          if (!canMutate || busy) return;
          const f = new FormData(e.currentTarget);
          setBusy(true);
          setError('');
          try {
            const result = await api.post<{ connectionId: string }>(
              '/state-protection/connections',
              {
                name: String(f.get('name') ?? ''),
                endpoint: String(f.get('endpoint') ?? ''),
                region: String(f.get('region') ?? ''),
                bucket: String(f.get('bucket') ?? ''),
                accessKey: String(f.get('access') ?? ''),
                secretKey: String(f.get('secret') ?? ''),
              },
            );
            if (!active.current) return;
            const prior = takeDestinationDraft() ?? {
              disk: '',
              connection: '',
              bucket: '',
              prefix: '',
              basic: false,
              third: false,
            };
            saveDestinationDraft({
              ...prior,
              connection: result.connectionId,
              bucket: String(f.get('bucket') ?? prior.bucket),
            });
            setDirty(false);
            setTimeout(() => {
              window.location.hash = '#/settings/state-protection';
            }, 0);
          } catch (e) {
            if (active.current) setError((e as Error).message);
          } finally {
            if (active.current) setBusy(false);
          }
        }}
      >
        {error ? <p role="alert">{error}</p> : null}
        <Field label="Connection name" name="name" required />
        <Field label="S3-compatible HTTPS endpoint" name="endpoint" type="url" required />
        <Field label="Region" name="region" defaultValue="us-east-1" required />
        <Field label="Bucket" name="bucket" required />
        <Field label="Access key" name="access" required autoComplete="off" />
        <Field label="Secret key" name="secret" type="password" required autoComplete="off" />
        <div className="actions">
          <Button type="submit" busy={busy} disabled={!canMutate}>
            Create and test connection
          </Button>
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => {
              window.location.hash = '#/settings/state-protection';
            }}
          >
            Cancel
          </Button>
        </div>
      </form>
    </section>
  );
}
