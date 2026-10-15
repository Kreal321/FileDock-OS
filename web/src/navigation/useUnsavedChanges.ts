import { useEffect, useRef } from 'react';
const guards = new Set<() => boolean>();
export function confirmNavigation() {
  return Array.from(guards).every((check) => check());
}
export function discardNavigationGuards() {
  guards.clear();
}
export function useUnsavedChanges(dirty: boolean) {
  const value = useRef(dirty);
  useEffect(() => {
    value.current = dirty;
  });
  useEffect(() => {
    const check = () => !value.current || window.confirm('Discard unsaved changes?');
    guards.add(check);
    const unload = (e: BeforeUnloadEvent) => {
      if (value.current) {
        e.preventDefault();
        e.returnValue = '';
      }
    };
    window.addEventListener('beforeunload', unload);
    return () => {
      guards.delete(check);
      window.removeEventListener('beforeunload', unload);
    };
  }, []);
}
