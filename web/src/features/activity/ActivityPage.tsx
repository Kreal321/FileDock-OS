import { EventsPage } from '../events/EventsPage.tsx';
import { resolveRoute } from '../../navigation/destinations.ts';
export function ActivityPage(props: { connected?: boolean; canMutate?: boolean }) {
  return <EventsPage route={resolveRoute('#/settings/activity')} {...props} />;
}
