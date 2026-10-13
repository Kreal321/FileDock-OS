import { act, renderHook, waitFor } from '@testing-library/react';
import { useHashRoute } from './useHashRoute.ts';
import { resolveRoute } from './destinations.ts';
test('resolves exact implemented routes and rejects unsupported views', () => {
  expect(resolveRoute('#/settings/network').id).toBe('network');
  expect(resolveRoute('#/pools').unavailable).toBe(true);
  expect(resolveRoute('#/settings/operations/op%20one').operationId).toBe('op one');
});
test('hash navigation responds to history events', async () => {
  window.location.hash = '#/dashboard';
  const { result } = renderHook(() => useHashRoute());
  act(() => {
    window.location.hash = '#/settings/audit';
    window.dispatchEvent(new Event('hashchange'));
  });
  await waitFor(() => expect(result.current.id).toBe('audit'));
});
