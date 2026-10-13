import type { CoreHealth } from '../api/events.ts';

export function CoreStatusBanner({ health }: { health?: CoreHealth }) {
  if (health?.canMutate) return null;
  return (
    <p className="warn" role="alert">
      {health?.detail ?? 'Checking the Core connection…'} Changes are unavailable until the
      connection is healthy.
    </p>
  );
}
