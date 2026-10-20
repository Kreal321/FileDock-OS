import { useEffect, useRef } from 'react';
import { PageHeader } from '../../components/PageHeader.tsx';
import { AlertsSection } from './AlertsPage.tsx';
import { NotificationChannelsSection } from './NotificationChannelsPage.tsx';
export function AlertsDeliveryPage({
  section = 'alerts',
  canMutate = false,
  connected = true,
}: {
  section?: string;
  canMutate?: boolean;
  connected?: boolean;
}) {
  const root = useRef<HTMLElement>(null);
  useEffect(() => {
    root.current?.querySelector<HTMLElement>(`#alerts-${section} h2`)?.focus();
  }, [section]);
  return (
    <section ref={root} className="stack">
      <PageHeader
        title="Alerts & delivery"
        description="Review appliance conditions and configure where alerts are delivered."
      />
      <nav aria-label="Alert sections" className="actions">
        <a href="#/settings/alerts">Alerts</a>
        <a href="#/settings/notification-channels">Delivery channels</a>
      </nav>
      <div id="alerts-alerts">
        <AlertsSection canMutate={canMutate} connected={connected} />
      </div>
      <div id="alerts-delivery">
        <NotificationChannelsSection canMutate={canMutate && connected} connected={connected} />
      </div>
    </section>
  );
}
