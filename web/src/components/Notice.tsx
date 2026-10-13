import type { ReactNode } from 'react';
import type { Tone } from '../api/view-models.ts';
import { Icon } from './Icon.tsx';
export function Notice({
  tone = 'warning',
  children,
  role,
}: {
  tone?: Tone;
  children: ReactNode;
  role?: 'alert' | 'status';
}) {
  return (
    <div className={`notice ${tone}`} role={role}>
      <Icon name={tone} />
      <div>{children}</div>
    </div>
  );
}
