import { useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Tabs } from '../../components/Tabs.tsx';
import { Notice } from '../../components/Notice.tsx';
import { Button } from '../../components/Button.tsx';
import type { Route } from '../../navigation/destinations.ts';
import {
  eventHash,
  rememberNextPage,
  resetPageHistory,
  type RecordType,
  type ViewFilters,
} from '../../navigation/viewContext.ts';
import { EventsToolbar } from './EventsToolbar.tsx';
import { RecordView } from './recordViews.tsx';
export function EventsPage({
  route,
  connected = true,
  canMutate = false,
}: {
  route: Route;
  connected?: boolean;
  canMutate?: boolean;
}) {
  const cache = useQueryClient();
  const type = route.recordType ?? 'activity';
  const filters = {
    ...(type === 'logs' ? { minSeverity: 'Info' } : {}),
    ...route.filters,
  } as ViewFilters;
  useEffect(() => {
    if (route.privateReset)
      window.history.replaceState(window.history.state, '', eventHash(type, route.filters ?? {}));
  }, [route.privateReset, type, route.filters]); // Keep the reset notice for this entry until the next navigation.
  const apply = (next: ViewFilters) => {
    resetPageHistory(type, next);
    const clean = { ...next };
    delete clean.pageToken;
    window.location.hash = eventHash(type, clean);
  };
  return (
    <section className="stack events-page">
      <PageHeader
        title="Events"
        description="Inspect appliance activity, service logs and administrative history."
      />
      {route.privateReset ? (
        <Notice tone="info" role="status">
          Private filters were cleared after reload. Re-enter them to refine these records.
        </Notice>
      ) : null}
      {route.filterError ? (
        <Notice tone="danger" role="alert">
          {route.filterError}{' '}
          <Button
            onClick={() => {
              window.location.hash = eventHash(type, {});
            }}
          >
            Reset filters
          </Button>
        </Notice>
      ) : null}
      <Tabs
        label="Event record type"
        selected={type}
        onSelect={(id) => {
          window.location.hash = `#/settings/${id}`;
        }}
        controls={
          <EventsToolbar
            key={type + JSON.stringify(filters)}
            type={type}
            filters={filters}
            apply={apply}
            refresh={() => void cache.invalidateQueries({ queryKey: [type] })}
          />
        }
        tabs={(['activity', 'logs', 'audit'] as RecordType[]).map((id) => ({
          id,
          label: { activity: 'Activity', logs: 'System logs', audit: 'Audit' }[id],
          content: route.filterError ? null : (
            <RecordView
              key={id + JSON.stringify(filters)}
              type={id}
              filters={filters}
              connected={connected}
              canMutate={canMutate}
              page={(cursor, direction) => {
                if (direction === 'next') rememberNextPage(type, filters, cursor);
                window.location.hash = eventHash(type, { ...filters, pageToken: cursor });
              }}
            />
          ),
        }))}
      />
    </section>
  );
}
