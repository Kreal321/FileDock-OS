import { fireEvent, render, screen } from '@testing-library/react';
import { Button } from './Button.tsx';
import { Field } from './Field.tsx';
import { Switch } from './Switch.tsx';
import { Tabs } from './Tabs.tsx';
import { Dialog } from './Dialog.tsx';
import { StatusBadge } from './StatusBadge.tsx';
import { DataTable } from './DataTable.tsx';
test('busy buttons and fields have accessible feedback', () => {
  render(
    <>
      <Button busy>Save</Button>
      <Field label="Username" name="username" error="Required" />
    </>,
  );
  expect(screen.getByRole('button')).toBeDisabled();
  expect(screen.getByLabelText('Username')).toHaveAttribute('aria-invalid', 'true');
  expect(screen.getByLabelText('Username')).toHaveAccessibleDescription('Required');
});
test('statuses are words plus an icon and switches expose state', () => {
  render(
    <>
      <StatusBadge state="Failed" />
      <Switch label="Enabled" checked={false} pending onChange={() => {}} />
    </>,
  );
  expect(screen.getByText('Failed')).toBeInTheDocument();
  expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'false');
  expect(screen.getByRole('switch')).toBeDisabled();
});
test('tabs use arrow keys and roving focus', () => {
  render(
    <Tabs
      tabs={[
        { id: 'one', label: 'One', content: 'First' },
        { id: 'two', label: 'Two', content: 'Second' },
      ]}
    />,
  );
  fireEvent.keyDown(screen.getByRole('tab', { name: 'One' }), { key: 'ArrowRight' });
  expect(screen.getByRole('tab', { name: 'Two' })).toHaveAttribute('aria-selected', 'true');
  expect(screen.getByRole('tabpanel')).toHaveTextContent('Second');
});
test('dialog Escape restores focus and table has a named region', () => {
  const close = vi.fn();
  const trigger = document.createElement('button');
  document.body.append(trigger);
  trigger.focus();
  render(
    <>
      <Dialog open title="Confirm" onClose={close}>
        <Button>Continue</Button>
      </Dialog>
      <DataTable label="History" headers={['Name']}>
        <tr>
          <td>Item</td>
        </tr>
      </DataTable>
    </>,
  );
  fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  expect(close).toHaveBeenCalled();
  expect(screen.getByRole('region', { name: 'History' })).toBeInTheDocument();
  trigger.remove();
});
