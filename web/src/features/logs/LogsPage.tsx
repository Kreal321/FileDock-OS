import { EventsPage } from '../events/EventsPage.tsx';
import { resolveRoute } from '../../navigation/destinations.ts';
export function LogsPage(props: { connected?: boolean; canMutate?: boolean }) {
  return <EventsPage route={resolveRoute('#/settings/logs')} {...props} />;
}
