import { QueryClient } from '@tanstack/react-query';
import { connectEvents } from './events.ts';
const mock = vi.hoisted(() => ({
  handlers: {} as Record<string, (...args: unknown[]) => void>,
  start: vi.fn(),
  stop: vi.fn(),
  reconnect: vi.fn(),
}));
vi.mock('@microsoft/signalr', () => ({
  LogLevel: { Warning: 1 },
  HttpError: class extends Error {},
  HubConnectionBuilder: class {
    withUrl() {
      return this;
    }
    withAutomaticReconnect() {
      return this;
    }
    configureLogging() {
      return this;
    }
    build() {
      return {
        on: (name: string, cb: (...args: unknown[]) => void) => {
          mock.handlers[name] = cb;
        },
        onreconnecting: (cb: () => void) => {
          mock.handlers.reconnecting = cb;
        },
        onreconnected: (cb: () => void) => {
          mock.handlers.reconnected = cb;
        },
        onclose: (cb: () => void) => {
          mock.handlers.close = cb;
        },
        start: mock.start,
        stop: mock.stop,
      };
    }
  },
}));
beforeEach(() => {
  vi.useFakeTimers();
  mock.start.mockReset().mockResolvedValue(undefined);
  mock.stop.mockReset().mockResolvedValue(undefined);
});
afterEach(() => vi.useRealTimers());
test('events invalidate views, health disables changes, reconnect resyncs, and duplicates are ignored', async () => {
  const cache = new QueryClient();
  const invalidate = vi.spyOn(cache, 'invalidateQueries');
  const stop = connectEvents(cache);
  await Promise.resolve();
  mock.handlers.event!({ seq: 5, type: 'subsystem.failed' });
  mock.handlers.event!({ seq: 5, type: 'subsystem.failed' });
  expect(invalidate).toHaveBeenCalledTimes(1);
  mock.handlers.reconnecting!();
  expect(cache.getQueryData(['events-connected'])).toBe(false);
  expect(cache.getQueryData<{ canMutate: boolean }>(['core-health'])?.canMutate).toBe(false);
  mock.handlers['core-health']!({ state: 'Healthy', canMutate: true, detail: 'connected' });
  expect(cache.getQueryData<{ canMutate: boolean }>(['core-health'])?.canMutate).toBe(true);
  mock.handlers.sync!();
  expect(invalidate).toHaveBeenCalledTimes(4);
  stop();
  expect(mock.stop).toHaveBeenCalledOnce();
});
test('initial connection failure is retried and disposal cancels retries', async () => {
  mock.start.mockRejectedValueOnce(new Error('offline'));
  const stop = connectEvents(new QueryClient());
  await vi.advanceTimersByTimeAsync(1000);
  expect(mock.start).toHaveBeenCalledTimes(2);
  stop();
  await vi.advanceTimersByTimeAsync(60000);
  expect(mock.start).toHaveBeenCalledTimes(2);
});

test('late reconnect callbacks cannot repopulate disposed session queries', async () => {
  const cache = new QueryClient();
  const dispose = connectEvents(cache);
  await Promise.resolve();
  dispose();
  cache.clear();
  mock.handlers.reconnected!();
  mock.handlers['core-health']!({ state: 'Healthy', canMutate: true, detail: 'late' });
  expect(cache.getQueryData(['events-connected'])).toBeUndefined();
  expect(cache.getQueryData(['core-health'])).toBeUndefined();
});
