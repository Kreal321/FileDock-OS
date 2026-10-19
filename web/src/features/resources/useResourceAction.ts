import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
export type Resource = {
  id: string;
  generation: number;
  spec: Record<string, unknown>;
  status?: Record<string, unknown>;
  effectiveConnectors?: Record<string, string>;
};
export type PageProps = { canMutate?: boolean; connected?: boolean };
export function useResourceAction(canMutate: boolean, connected: boolean, keys: string[]) {
  const cache = useQueryClient();
  const active = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [operation, setOperation] = useState('');
  async function act(action: () => Promise<unknown>, done?: () => void) {
    if (!canMutate || !connected || active.current) return;
    active.current = true;
    setBusy(true);
    setError('');
    try {
      const value = await action();
      if (value && typeof value === 'object' && 'id' in value && 'state' in value)
        setOperation(String(value.id));
      done?.();
      for (const key of keys) await cache.invalidateQueries({ queryKey: [key] });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'The request failed.');
    } finally {
      active.current = false;
      setBusy(false);
    }
  }
  return { busy, error, operation, act, setError, disabled: busy || !canMutate || !connected };
}
