import { useSyncExternalStore } from 'react';
import { resolveRoute } from './destinations.ts';
import { confirmNavigation, discardNavigationGuards } from './useUnsavedChanges.ts';
const listeners = new Set<() => void>();
let hash = '';
let index = 0;
let restoring = false;
let bypass = false;
function stamp() {
  window.history.replaceState({ ...window.history.state, fdNavigationIndex: index }, '');
}
function changed() {
  const incoming = window.location.hash;
  if (restoring) {
    if (incoming === hash) {
      restoring = false;
    }
    return;
  }
  if (incoming === hash) return;
  const next = window.history.state?.fdNavigationIndex as number | undefined;
  if (!bypass && !confirmNavigation()) {
    restoring = true;
    const delta = next === undefined ? -1 : index - next;
    if (delta) window.history.go(delta);
    else {
      window.history.replaceState(window.history.state, '', hash);
      restoring = false;
    }
    return;
  }
  bypass = false;
  hash = incoming;
  index = next ?? index + 1;
  stamp();
  listeners.forEach((fn) => fn());
}
const subscribe = (listener: () => void) => {
  if (!listeners.size) {
    hash = window.location.hash;
    index = window.history.state?.fdNavigationIndex ?? 0;
    stamp();
    window.addEventListener('hashchange', changed);
  }
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (!listeners.size) {
      window.removeEventListener('hashchange', changed);
      restoring = false;
    }
  };
};
export function useHashRoute() {
  return resolveRoute(
    useSyncExternalStore(
      subscribe,
      () => (listeners.size ? hash : window.location.hash),
      () => '',
    ),
  );
}
export function resetNavigation() {
  discardNavigationGuards();
  restoring = false;
  bypass = true;
  window.history.replaceState(
    { ...window.history.state, fdNavigationIndex: 0 },
    '',
    `${window.location.pathname}${window.location.search}#/dashboard`,
  );
  changed();
  bypass = false;
  window.dispatchEvent(new Event('hashchange'));
}
