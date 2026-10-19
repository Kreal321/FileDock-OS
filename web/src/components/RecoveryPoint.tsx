import { displayTime } from '../api/view-models.ts';

// An absent or malformed observation must never look like a successful backup.
export function RecoveryPoint({
  at,
  cached = false,
  label = 'Latest durable recovery point',
}: {
  at?: string | null;
  cached?: boolean;
  label?: string;
}) {
  const valid = Boolean(at && Number.isFinite(Date.parse(at)));
  return (
    <p className="caption muted">
      {label}:{' '}
      {valid ? (
        <time dateTime={at!}>{displayTime(at!)}</time>
      ) : (
        'Not reported — recovery coverage is unverified.'
      )}
      {cached && ' · Cached observation'}
    </p>
  );
}
