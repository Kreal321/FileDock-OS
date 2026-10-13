import { HubConnectionBuilder, HttpError, LogLevel } from '@microsoft/signalr';
import type { QueryClient } from '@tanstack/react-query';

export type CoreHealth = {
  state: string;
  detail: string;
  interfaceVersion?: string;
  canMutate: boolean;
};
export type CoreEvent = {
  seq: number;
  type: string;
  subjectKind?: string;
  subjectId?: string;
  payload: unknown;
};

export function connectEvents(cache: QueryClient) {
  let disposed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let delay = 1000;
  let lastSeq = 0;
  const connection = new HubConnectionBuilder()
    .withUrl('/hubs/events')
    .withAutomaticReconnect({
      nextRetryDelayInMilliseconds: (context) =>
        Math.min(1000 * 2 ** Math.min(context.previousRetryCount, 5), 30000),
    })
    .configureLogging(LogLevel.Warning)
    .build();
  const unavailable = () => {
    if (disposed) return;
    cache.setQueryData(['events-connected'], false);
    cache.setQueryData<CoreHealth>(['core-health'], {
      state: 'Unreachable',
      detail: 'Live connection lost. Reconnecting automatically…',
      canMutate: false,
    });
  };
  const refresh = () =>
    void cache.invalidateQueries({
      predicate: (query) =>
        !['session-gate', 'core-health', 'events-connected'].includes(String(query.queryKey[0])),
    });
  connection.on('event', (event: CoreEvent) => {
    if (disposed || event.seq <= lastSeq) return;
    lastSeq = event.seq;
    refresh();
  });
  connection.on('core-health', (health: CoreHealth) => {
    if (disposed) return;
    cache.setQueryData(['core-health'], health);
    void cache.invalidateQueries({ queryKey: ['system-status'] });
  });
  connection.on('sync', () => {
    if (!disposed) {
      refresh();
      void cache.invalidateQueries({ queryKey: ['core-health'] });
    }
  });
  connection.on('session-expired', () => {
    if (!disposed) window.dispatchEvent(new Event('filedock:session-expired'));
  });
  connection.onreconnecting(unavailable);
  connection.onreconnected(() => {
    cache.setQueryData(['events-connected'], true);
    delay = 1000;
    refresh();
  });
  connection.onclose(() => {
    if (!disposed) {
      unavailable();
      timer = setTimeout(() => void start(), delay);
    }
  });
  async function start() {
    if (disposed) return;
    try {
      await connection.start();
      if (!disposed) cache.setQueryData(['events-connected'], true);
      delay = 1000;
    } catch (error) {
      if (disposed) return;
      unavailable();
      if (error instanceof HttpError && error.statusCode === 401) {
        window.dispatchEvent(new Event('filedock:session-expired'));
        return;
      }
      timer = setTimeout(() => void start(), delay);
      delay = Math.min(delay * 2, 30000);
    }
  }
  void start();
  return () => {
    disposed = true;
    clearTimeout(timer);
    void connection.stop();
  };
}
