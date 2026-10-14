import { destinations } from '../../navigation/destinations.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Icon } from '../../components/Icon.tsx';
const descriptions: Record<string, string> = {
  administrators: 'Manage accounts and your password.',
  tokens: 'Connect applications with scoped access.',
  network: 'Choose local HTTP listener addresses.',
  operations: 'Inspect Core work and operation steps.',
  audit: 'Review administrative changes.',
  logs: 'Filter service and subsystem logs.',
  activity: 'Follow recent appliance events.',
  alerts: 'Review and acknowledge active conditions.',
  'notification-channels': 'Choose where extensions deliver alerts.',
  'state-protection': 'Check configuration recovery copies.',
  locations: 'Review owned recovery locations.',
};
export function SettingsPage() {
  return (
    <div className="stack">
      <PageHeader
        title="Settings"
        description="Manage access, connectivity, and appliance activity."
      />
      <div className="settings-cards">
        {destinations
          .filter((d) => d.group === 'settings')
          .map((d) => (
            <a key={d.id} className="panel destination-card" href={d.hashRoute}>
              <span className="icon-tile">
                <Icon name={d.icon} />
              </span>
              <div>
                <h2>{d.label}</h2>
                <p>{descriptions[d.id]}</p>
              </div>
              <Icon name="arrow" />
            </a>
          ))}
      </div>
    </div>
  );
}
