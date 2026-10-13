import type { HTMLAttributes } from 'react';
export function Panel({ className = '', children, ...props }: HTMLAttributes<HTMLElement>) {
  return (
    <section className={`panel ${className}`} {...props}>
      {children}
    </section>
  );
}
