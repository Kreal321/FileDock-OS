import { statusPresentation } from '../api/view-models.ts';
import { Icon } from './Icon.tsx';
export function StatusBadge({
  state,
  freshness = 'current',
}: {
  state: string;
  freshness?: 'current' | 'cached' | 'unavailable';
}) {
  const view = statusPresentation(state, freshness);
  return (
    <span className={`badge ${view.tone}`}>
      <Icon name={view.tone} />
      {view.label}
      {freshness === 'cached' ? ' · Cached' : freshness === 'unavailable' ? ' · Unavailable' : ''}
    </span>
  );
}
