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

test('dirty route guard protects history changes and reset bypasses it on signout', async () => {
  const { useUnsavedChanges } = await import('./useUnsavedChanges.ts');
  const { resetNavigation } = await import('./useHashRoute.ts');
  window.history.replaceState(null, '', '#/dashboard');
  const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
  const { result, unmount } = renderHook(() => {
    useUnsavedChanges(true);
    return useHashRoute();
  });
  act(() => {
    window.location.hash = '#/settings/audit';
    window.dispatchEvent(new Event('hashchange'));
  });
  await waitFor(() => expect(window.location.hash).toBe('#/dashboard'));
  expect(result.current.id).toBe('dashboard');
  confirm.mockReturnValue(true);
  act(() => {
    window.location.hash = '#/settings/audit';
    window.dispatchEvent(new Event('hashchange'));
  });
  await waitFor(() => expect(result.current.id).toBe('audit'));
  confirm.mockReturnValue(false);
  act(() => resetNavigation());
  expect(window.location.hash).toBe('#/dashboard');
  unmount();
  confirm.mockRestore();
});
