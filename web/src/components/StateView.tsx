import type { ReactNode } from 'react';
import type { DataView } from '../api/view-models.ts';
import { Button } from './Button.tsx';
import { Notice } from './Notice.tsx';
export function StateView({
  phase,
  error,
  onRetry,
  empty = 'No records yet.',
  children,
}: {
  phase: DataView<unknown>['phase'];
  error?: string;
  onRetry?: () => void;
  empty?: string;
  children?: ReactNode;
}) {
  return (
    <>
      {phase === 'loading' ? <p role="status">Loading…</p> : null}
      {phase === 'empty' ? <p className="muted">{empty}</p> : null}
      {['error', 'unavailable', 'stale'].includes(phase) ? (
        <Notice tone={phase === 'stale' ? 'warning' : 'danger'} role="status">
          <p>
            {phase === 'stale' ? 'Showing cached data. ' : 'Data unavailable. '}
            {error}
          </p>
          {onRetry ? (
            <Button variant="secondary" onClick={onRetry}>
              Retry
            </Button>
          ) : null}
        </Notice>
      ) : null}
      {children}
    </>
  );
}
