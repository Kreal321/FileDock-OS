import { fireEvent, render, screen } from '@testing-library/react';
import { EventsToolbar } from './EventsToolbar.tsx';
test('source commits on Enter and extra filters stay in a deliberate dialog', () => {
  const apply = vi.fn(),
    refresh = vi.fn();
  render(
    <EventsToolbar type="logs" filters={{ minSeverity: 'Info' }} apply={apply} refresh={refresh} />,
  );
  fireEvent.change(screen.getByLabelText('Source'), { target: { value: 'filedock-core.service' } });
  expect(apply).not.toHaveBeenCalled();
  fireEvent.submit(screen.getByLabelText('Source').closest('form')!);
  expect(apply).toHaveBeenCalledWith({ minSeverity: 'Info', source: 'filedock-core.service' });
  fireEvent.click(screen.getByRole('button', { name: 'Filters' }));
  expect(screen.getByRole('dialog', { name: 'Filter system logs' })).toBeVisible();
  fireEvent.change(screen.getByLabelText('Since (ISO timestamp)'), { target: { value: 'bad' } });
  fireEvent.click(screen.getByRole('button', { name: 'Apply filters' }));
  expect(screen.getByRole('alert')).toHaveTextContent('ISO');
});
