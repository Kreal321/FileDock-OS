import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { effectiveCanMutate } from './view-models.ts';
import { api } from './client.ts';
import { connectEvents, type CoreHealth } from './events.ts';

export function useLiveCoreStatus() {
  const cache = useQueryClient();
  useEffect(() => connectEvents(cache), [cache]);
  const query = useQuery({
    queryKey: ['core-health'],
    queryFn: () => api.get<CoreHealth>('/system/core-health'),
    retry: false,
  });
  const live = useQuery({
    queryKey: ['events-connected'],
    queryFn: async () => false,
    enabled: false,
    initialData: false,
  });
  if (!live.data)
    return {
      browserConnected: false,
      state: 'Unreachable',
      detail: 'Live connection unavailable. Reconnecting automatically…',
      canMutate: false,
    };
  return query.error
    ? {
        browserConnected: true,
        state: 'Unreachable',
        detail: 'Cannot reach the appliance. Reconnecting automatically…',
        canMutate: false,
      }
    : query.data
      ? {
          ...query.data,
          browserConnected: true,
          canMutate: effectiveCanMutate(live.data, query.data),
        }
      : {
          browserConnected: true,
          state: 'Starting',
          detail: 'Checking the Core connection…',
          canMutate: false,
        };
}
