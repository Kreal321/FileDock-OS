import { EventsPage } from '../events/EventsPage.tsx';
import { resolveRoute } from '../../navigation/destinations.ts';
export function AuditPage(props: { connected?: boolean; canMutate?: boolean }) {
  return <EventsPage route={resolveRoute('#/settings/audit')} {...props} />;
}
