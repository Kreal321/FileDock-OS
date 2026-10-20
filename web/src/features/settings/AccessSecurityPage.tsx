import { useEffect, useRef } from 'react';
import { PageHeader } from '../../components/PageHeader.tsx';
import { AdminsSection } from '../admins/AdminsPage.tsx';
import { TokensSection } from '../tokens/TokensPage.tsx';
export function AccessSecurityPage({
  section = 'administrators',
  ...props
}: {
  section?: string;
  canMutate?: boolean;
  connected?: boolean;
  onSignedOut: () => void;
}) {
  const root = useRef<HTMLElement>(null);
  useEffect(() => {
    root.current?.querySelector<HTMLElement>(`#access-${section} h2`)?.focus();
  }, [section]);
  return (
    <section ref={root} className="stack">
      <PageHeader
        title="Access & security"
        description="Manage appliance administrators, your password and scoped application tokens."
      />
      <nav aria-label="Account sections" className="actions">
        <a href="#/settings/administrators">Administrators</a>
        <a href="#/settings/tokens">API tokens</a>
      </nav>
      <div id="access-administrators">
        <AdminsSection {...props} />
      </div>
      <div id="access-tokens">
        <TokensSection connected={props.connected} canMutate={props.canMutate} />
      </div>
    </section>
  );
}
