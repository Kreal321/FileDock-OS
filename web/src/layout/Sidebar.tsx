import { destinations, type Route } from '../navigation/destinations.ts';
import { Icon } from '../components/Icon.tsx';
export function Sidebar({
  route,
  open,
  onClose,
}: {
  route: Route;
  open: boolean;
  onClose: () => void;
}) {
  return (
    <aside className={`sidebar ${open ? 'mobile-open' : ''}`}>
      <a className="brand" href="#/dashboard">
        <Icon name="folder" />
        FileDock
      </a>
      <nav
        id="main-navigation"
        aria-label="Main navigation"
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            onClose();
          }
        }}
      >
        {destinations
          .filter((d) => d.group === 'main')
          .map((d) => (
            <a
              key={d.id}
              className="nav-link"
              href={d.hashRoute}
              aria-current={
                (d.id === 'dashboard' ? route.id === 'dashboard' : route.id !== 'dashboard')
                  ? 'page'
                  : undefined
              }
              onClick={onClose}
            >
              <Icon name={d.icon} />
              {d.label}
            </a>
          ))}
      </nav>
      <p className="sidebar-footer">
        FileDock OS
        <br />
        Appliance management
      </p>
    </aside>
  );
}
