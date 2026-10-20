/** Signed-in memory only. Never serialize private values into history or browser storage. */
export type RecordType = 'activity' | 'logs' | 'audit';
export type ViewFilters = Record<string, string>;
const contexts = new Map<string, { type: RecordType; filters: ViewFilters }>();
export function saveViewContext(type: RecordType, filters: ViewFilters): string {
  // getRandomValues is available on the appliance's HTTP origin; randomUUID is secure-context-only.
  const id = Array.from(crypto.getRandomValues(new Uint8Array(16)), (byte) =>
    byte.toString(16).padStart(2, '0'),
  ).join('');
  if (contexts.size >= 250) contexts.delete(contexts.keys().next().value!);
  contexts.set(id, { type, filters: { ...filters } });
  return id;
}
export function readViewContext(id: string, type: RecordType): ViewFilters | undefined {
  const value = contexts.get(id);
  return value?.type === type ? { ...value.filters } : undefined;
}
export function clearViewContexts() {
  contexts.clear();
  paging.clear();
}
export function validateDates(filters: ViewFilters): string | undefined {
  for (const key of ['since', 'until']) {
    const value = filters[key];
    if (!value) continue;
    const date = value.slice(0, 10);
    if (
      !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(?::\d{2}(?:\.\d{1,3})?)?(?:Z|[+-]\d{2}:\d{2})$/.test(
        value,
      ) ||
      !Number.isFinite(Date.parse(value)) ||
      new Date(`${date}T00:00:00Z`).toISOString().slice(0, 10) !== date
    )
      return `${key === 'since' ? 'Since' : 'Until'} must be an ISO timestamp.`;
  }
  if (filters.since && filters.until && Date.parse(filters.since) > Date.parse(filters.until))
    return 'Since must be before or equal to Until.';
}
export const privateFields: Record<RecordType, string[]> = {
  activity: [],
  logs: ['source', 'text'],
  audit: ['actor', 'action', 'target', 'result'],
};
export function eventHash(type: RecordType, filters: ViewFilters): string {
  const query = new URLSearchParams();
  const privateValues: ViewFilters = {};
  for (const [key, value] of Object.entries(filters)) {
    if (!value) continue;
    if (privateFields[type].includes(key)) privateValues[key] = value;
    else query.set(key, value);
  }
  if (Object.keys(privateValues).length) query.set('context', saveViewContext(type, privateValues));
  return `#/settings/${type}${query.size ? '?' + query : ''}`;
}

const paging = new Map<string, Map<string, string>>();
function pagingKey(type: RecordType, filters: ViewFilters) {
  return JSON.stringify([
    type,
    Object.entries(filters)
      .filter(([key, value]) => key !== 'pageToken' && value)
      .sort(([a], [b]) => a.localeCompare(b)),
  ]);
}
export function rememberNextPage(type: RecordType, filters: ViewFilters, next: string) {
  const key = pagingKey(type, filters);
  if (!paging.has(key)) {
    if (paging.size >= 250) paging.delete(paging.keys().next().value!);
    paging.set(key, new Map());
  }
  const history = paging.get(key)!;
  if (history.size >= 500) history.delete(history.keys().next().value!);
  history.set(next, filters.pageToken ?? '');
}
export function previousPageCursor(type: RecordType, filters: ViewFilters): string | undefined {
  return filters.pageToken
    ? paging.get(pagingKey(type, filters))?.get(filters.pageToken)
    : undefined;
}
export function resetPageHistory(type: RecordType, filters: ViewFilters) {
  paging.delete(pagingKey(type, filters));
}
