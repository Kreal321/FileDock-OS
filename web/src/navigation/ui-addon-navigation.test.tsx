import { render } from '@testing-library/react';
import { Sidebar } from '../layout/Sidebar.tsx';
import { destinations, resolveRoute } from './destinations.ts';
test.each(destinations.map((d) => [d.hashRoute]))('one current leaf for %s', (hash) => {
  const route = resolveRoute(hash!);
  const { container } = render(<Sidebar route={route} open onClose={() => {}} />);
  expect(container.querySelectorAll('[aria-current=page]')).toHaveLength(1);
  const settings = container.querySelector('[data-group=settings-group]')!;
  expect(settings.querySelectorAll('a')).toHaveLength(3);
  expect(container.querySelector('[aria-current=page]')).toHaveAttribute(
    'href',
    expect.any(String),
  );
  if (route.groupId)
    expect(container.querySelector(`[data-group=${route.groupId}]`)).toHaveAttribute('open');
});
test.each([
  ['#/folders/new', 'folders'],
  ['#/folders/id%20one/edit', 'folders'],
  ['#/extensions/ext%20one/upgrade', 'extensions'],
  ['#/extensions/package', 'extensions'],
  ['#/settings/state-protection/connection', 'state-protection'],
  ['#/settings/operations/op%20one', 'operations'],
])('editor/detail %s has owner %s', (hash, owner) => expect(resolveRoute(hash!).owner).toBe(owner));
