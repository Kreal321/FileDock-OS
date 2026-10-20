import { useId, useState } from 'react';
import { Button } from '../../components/Button.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import { Field } from '../../components/Field.tsx';
import { Select } from '../../components/Select.tsx';
import { validateDates, type RecordType, type ViewFilters } from '../../navigation/viewContext.ts';
export function EventsToolbar({
  type,
  filters,
  apply,
  refresh,
}: {
  type: RecordType;
  filters: ViewFilters;
  apply: (filters: ViewFilters) => void;
  refresh: () => void;
}) {
  const [open, setOpen] = useState(false),
    [error, setError] = useState('');
  const formId = useId();
  const extras =
    type === 'logs'
      ? ['since', 'until', 'text']
      : type === 'audit'
        ? ['actor', 'action', 'target', 'result', 'since', 'until']
        : [];
  const labels: Record<string, string> = {
    since: 'Since (ISO timestamp)',
    until: 'Until (ISO timestamp)',
    text: 'Containing text',
    actor: 'Actor',
    action: 'Action',
    target: 'Target',
    result: 'Result',
  };
  const count = extras.filter((key) => filters[key]).length;
  return (
    <>
      {type === 'logs' ? (
        <>
          <form
            className="event-source"
            onSubmit={(e) => {
              e.preventDefault();
              apply({
                ...filters,
                source: String(new FormData(e.currentTarget).get('source') ?? ''),
              });
            }}
          >
            <Field
              key={filters.source ?? ''}
              label="Source"
              name="source"
              defaultValue={filters.source ?? ''}
              placeholder="All services"
            />
          </form>
          <Select
            label="Minimum severity"
            value={filters.minSeverity ?? 'Info'}
            onChange={(e) => apply({ ...filters, minSeverity: e.target.value })}
          >
            {['Debug', 'Info', 'Warning', 'Error', 'Critical'].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </Select>
        </>
      ) : null}
      {extras.length ? (
        <Button
          variant="secondary"
          onClick={() => {
            setError('');
            setOpen(true);
          }}
        >
          Filters{count ? ` (${count})` : ''}
        </Button>
      ) : null}
      <Button variant="secondary" onClick={refresh}>
        Refresh
      </Button>
      <Dialog
        open={open}
        title={type === 'logs' ? 'Filter system logs' : 'Filter audit records'}
        onClose={() => setOpen(false)}
        footer={
          <Button form={formId} type="submit">
            Apply filters
          </Button>
        }
      >
        {error ? <p role="alert">{error}</p> : null}
        <form
          id={formId}
          className="stack"
          onSubmit={(e) => {
            e.preventDefault();
            const data = new FormData(e.currentTarget);
            const next = {
              ...filters,
              ...Object.fromEntries(extras.map((key) => [key, String(data.get(key) ?? '')])),
            };
            const problem = validateDates(next);
            if (problem) {
              setError(problem);
              return;
            }
            apply(next);
            setOpen(false);
          }}
        >
          {extras.map((key) => (
            <Field key={key} name={key} label={labels[key]!} defaultValue={filters[key] ?? ''} />
          ))}
        </form>
      </Dialog>
    </>
  );
}
