import type { SVGProps } from 'react';
const paths: Record<string, string> = {
  storage: 'M4 4h16v6H4z M4 14h16v6H4z M7 7h1 M7 17h1 M11 7h6 M11 17h6',
  cloud: 'M7 18a5 5 0 1 1 0-10 6 6 0 0 1 11-1 5 5 0 1 1 1 11H7z',
  extension: 'M8 3h5v4a3 3 0 1 1 0 6v4H8a3 3 0 1 0-6 0H2V3h6z',
  folder: 'M3 7h7l2 2h9v11H3z M3 7V5h7l2 2',
  dashboard: 'M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z',
  settings:
    'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8 M9 3h6l1 3 3 1 2 5-2 5-3 1-1 3H9l-1-3-3-1-2-5 2-5 3-1z',
  service: 'M4 3h16v7H4z M4 14h16v7H4z M7 6h1 M7 17h1 M11 6h6 M11 17h6',
  success: 'M5 12l4 4L19 6',
  warning: 'M12 3L2 21h20z M12 9v5 M12 17v1',
  danger: 'M6 6l12 12 M18 6L6 18',
  info: 'M12 11v7 M12 6v1',
  neutral: 'M5 12h14',
  operations: 'M4 5h16 M4 12h16 M4 19h16',
  audit: 'M5 3h14v18H5z M8 7h8 M8 11h8 M8 15h5',
  admin: 'M12 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8 M4 21v-3a8 6 0 0 1 16 0v3',
  token: 'M8 4a5 5 0 1 0 0 10 5 5 0 0 0 0-10 M12 12l9 9 M16 16l3-3 M18 18l3-3',
  network: 'M9 3h6v6H9z M2 16h6v5H2z M16 16h6v5h-6z M12 9v4 M5 16v-3h14v3',
  menu: 'M3 6h18 M3 12h18 M3 18h18',
  arrow: 'M9 5l7 7-7 7',
  download: 'M12 3v12 M7 10l5 5 5-5 M4 17v4h16v-4',
  signout: 'M9 3H3v18h6 M9 12h12 M16 7l5 5-5 5',
};
export function Icon({
  name = 'folder',
  title,
  ...props
}: SVGProps<SVGSVGElement> & { name?: string; title?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width="24"
      height="24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden={title ? undefined : true}
      role={title ? 'img' : undefined}
      aria-label={title}
      {...props}
    >
      <path d={paths[name] ?? paths.service} />
    </svg>
  );
}
