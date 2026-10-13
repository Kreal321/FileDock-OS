import type { ReactNode } from 'react';
import { Icon } from '../components/Icon.tsx';
export function AuthLayout({ children }: { children: ReactNode }) {
  return (
    <div className="auth-layout">
      <div className="brand">
        <Icon name="folder" />
        FileDock
      </div>
      <main className="auth-content">{children}</main>
      <footer>
        Appliance management · <a href="/assets/licenses/inter-OFL.txt">Font license</a>
      </footer>
    </div>
  );
}
