import { destinations } from '../../navigation/destinations.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { Icon } from '../../components/Icon.tsx';
const descriptions: Record<string, string> = {
  administrators: 'Manage accounts and your password.',
  tokens: 'Connect applications with scoped access.',
  connectors: 'Choose cloud connectors, defaults, and connection overrides.',
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
const groups = [
  {
    title: 'Access and connectivity',
    ids: ['administrators', 'tokens', 'network', 'connectors', 'notification-channels'],
  },
  { title: 'Recovery', ids: ['state-protection', 'locations'] },
  {
    title: 'Operations and diagnostics',
    ids: ['operations', 'activity', 'alerts', 'logs', 'audit'],
  },
];
export function SettingsPage() {
  return (
    <section className="stack">
      <PageHeader
        title="Settings"
        description="Manage access, connectivity, and appliance activity."
      />
      {groups.map((group) => (
        <Panel key={group.title}>
          <h2 className="settings-group-title">{group.title}</h2>
          {group.ids.map((id) => {
            const d = destinations.find((d) => d.id === id);
            return d ? (
              <a key={id} className="settings-row settings-link" href={d.hashRoute}>
                <Icon name={d.icon} />
                <div className="settings-row-content">
                  <h3>{d.label}</h3>
                  <p className="muted">{descriptions[id]}</p>
                </div>
                <span aria-hidden="true">›</span>
              </a>
            ) : null;
          })}
        </Panel>
      ))}
    </section>
  );
}
