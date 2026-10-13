import type { CoreHealth } from '../api/events.ts';
import { Notice } from './Notice.tsx';
export function CoreStatusBanner({ health }: { health?: CoreHealth }) {
  if (health?.canMutate) return null;
  return (
    <Notice role="alert" tone="warning">
      {health?.detail ?? 'Checking the Core connection…'} Changes are unavailable until the
      connection is healthy. Inspection and sign-out remain available.
    </Notice>
  );
}
