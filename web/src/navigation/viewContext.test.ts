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
