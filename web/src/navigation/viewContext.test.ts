import { clearViewContexts, saveViewContext, readViewContext } from './viewContext.ts';
import { destinations, resolveRoute } from './destinations.ts';
afterEach(clearViewContexts);
test('all twenty existing entries retain identities and a navigation owner', () => {
  expect(destinations).toHaveLength(20);
  for (const entry of destinations) {
    expect(resolveRoute(entry.hashRoute).id).toBe(entry.id);
    expect(resolveRoute(entry.hashRoute).owner).toBeTruthy();
  }
});
test('filters are private memory by type and expire on logout', () => {
  const ref = saveViewContext('logs', { text: 'private-marker' });
  expect(ref).not.toContain('private-marker');
  expect(readViewContext(ref, 'logs')).toEqual({ text: 'private-marker' });
  expect(readViewContext(ref, 'audit')).toBeUndefined();
  clearViewContexts();
  expect(readViewContext(ref, 'logs')).toBeUndefined();
  expect(resolveRoute(`#/settings/logs?context=${ref}&pageToken=old`).privateReset).toBe(true);
});
test.each([
  '#/settings/logs?text=unsafe',
  '#/settings/logs?pageSize=501',
  '#/settings/activity?minSeverity=Info',
  '#/settings/logs?since=nope',
  '#/settings/audit?since=2026-10-20T00%3A00%3A00Z&until=2026-10-19T00%3A00%3A00Z',
])('invalid filter query is explicit: %s', (hash) =>
  expect(resolveRoute(hash).filterError).toBeTruthy(),
);
test('queries preserve intended type and public filters', () => {
  const route = resolveRoute('#/settings/logs?minSeverity=Warning&pageSize=50');
  expect(route.id).toBe('logs');
  expect(route.owner).toBe('events');
  expect(route.filters).toEqual({ minSeverity: 'Warning', pageSize: '50' });
});
test('paging history is scoped to type and applied filters and clears with the session', async () => {
  const { rememberNextPage, previousPageCursor, resetPageHistory } =
    await import('./viewContext.ts');
  const filters = { minSeverity: 'Info', source: 'private-source' };
  expect(previousPageCursor('logs', { ...filters, pageToken: 'direct-entry' })).toBeUndefined();
  rememberNextPage('logs', filters, 'second');
  expect(previousPageCursor('logs', { ...filters, pageToken: 'second' })).toBe('');
  expect(
    previousPageCursor('logs', { ...filters, minSeverity: 'Warning', pageToken: 'second' }),
  ).toBeUndefined();
  expect(previousPageCursor('audit', { ...filters, pageToken: 'second' })).toBeUndefined();
  resetPageHistory('logs', filters);
  expect(previousPageCursor('logs', { ...filters, pageToken: 'second' })).toBeUndefined();
  rememberNextPage('logs', filters, 'second');
  clearViewContexts();
  expect(previousPageCursor('logs', { ...filters, pageToken: 'second' })).toBeUndefined();
});
test('private references work on HTTP without secure-context randomUUID', () => {
  const original = globalThis.crypto;
  vi.stubGlobal('crypto', { getRandomValues: original.getRandomValues.bind(original) });
  try {
    const id = saveViewContext('logs', { source: 'private' });
    expect(id).toMatch(/^[0-9a-f]{32}$/);
    expect(readViewContext(id, 'logs')).toEqual({ source: 'private' });
  } finally {
    vi.unstubAllGlobals();
  }
});
