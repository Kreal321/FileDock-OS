import { useState } from 'react';
import { api } from '../api/client.ts';
import type { AuthenticatedIdentity } from '../api/view-models.ts';
import { Button } from '../components/Button.tsx';
import { Icon } from '../components/Icon.tsx';
export function TopBar({
  identity,
  onSignOut,
  open,
  onToggle,
  toggleRef,
}: {
  identity?: AuthenticatedIdentity;
  onSignOut: () => void;
  open: boolean;
  onToggle: () => void;
  toggleRef: React.RefObject<HTMLButtonElement | null>;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  return (
    <>
      <header className="topbar">
        <Button
          ref={toggleRef}
          className="mobile-toggle"
          variant="secondary"
          aria-label="Open navigation"
          aria-expanded={open}
          aria-controls="main-navigation"
          onClick={onToggle}
        >
          <Icon name="menu" />
        </Button>
        <span className="topbar-title">Management</span>
        <div className="account">
          {identity ? (
            <>
              <span className="avatar" aria-hidden="true">
                {identity.displayInitials}
              </span>
              <span className="account-name">{identity.username}</span>
            </>
          ) : (
            <span className="caption">Loading account…</span>
          )}
          <Button
            variant="secondary"
            busy={busy}
            onClick={async () => {
              setBusy(true);
              setError('');
              try {
                await api.post('/auth/sign-out');
                onSignOut();
              } catch (err) {
                setError(err instanceof Error ? err.message : 'Could not sign out.');
              } finally {
                setBusy(false);
              }
            }}
          >
            <Icon name="signout" />
            Sign out
          </Button>
        </div>
      </header>
      {error ? (
        <p className="notice danger" role="alert">
          {error}
        </p>
      ) : null}
    </>
  );
}
