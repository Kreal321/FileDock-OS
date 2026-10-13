import { useSyncExternalStore } from 'react';
import { resolveRoute } from './destinations.ts';
const subscribe = (listener: () => void) => {
  window.addEventListener('hashchange', listener);
  return () => window.removeEventListener('hashchange', listener);
};
export function useHashRoute() {
  return resolveRoute(
    useSyncExternalStore(
      subscribe,
      () => window.location.hash,
      () => '',
    ),
  );
}
export function resetNavigation() {
  window.history.replaceState(
    null,
    '',
    `${window.location.pathname}${window.location.search}#/dashboard`,
  );
  window.dispatchEvent(new Event('hashchange'));
}
