import {
  Children,
  cloneElement,
  isValidElement,
  type ReactElement,
  type ReactNode,
  type CSSProperties,
} from 'react';
export type TableColumn = {
  id: string;
  label: string;
  align?: 'start' | 'end' | 'actions';
  width?: string;
  minWidth?: string;
};
type Cell = { children?: ReactNode; style?: CSSProperties; 'data-align'?: string };
export function DataTable({
  label,
  headers,
  children,
}: {
  label: string;
  headers: (string | TableColumn)[];
  children: ReactNode;
}) {
  const columns = headers.map((h) =>
    typeof h === 'string'
      ? { id: h, label: h, align: h === 'Actions' ? ('actions' as const) : ('start' as const) }
      : h,
  );
  return (
    <div className="table-region" role="region" aria-label={label} tabIndex={0}>
      <table style={{ '--table-column-count': columns.length } as CSSProperties}>
        <caption className="sr-only">{label}</caption>
        <colgroup>
          {columns.map((c) => (
            <col
              key={c.id}
              style={{ width: c.width ?? (c.align === 'actions' ? '96px' : undefined) }}
            />
          ))}
        </colgroup>
        <thead>
          <tr>
            {columns.map((c) => (
              <th
                key={c.id}
                scope="col"
                data-align={c.align ?? 'start'}
                style={{ minWidth: c.minWidth }}
              >
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {Children.map(children, (row) => {
            if (!isValidElement<Cell>(row)) return row;
            return cloneElement(
              row,
              {},
              Children.map(row.props.children, (cell, i) =>
                isValidElement<Cell>(cell)
                  ? cloneElement(cell as ReactElement<Cell>, {
                      'data-align': columns[i]?.align ?? 'start',
                      style: { ...cell.props.style, minWidth: columns[i]?.minWidth },
                    })
                  : cell,
              ),
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
