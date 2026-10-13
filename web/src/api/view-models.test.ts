import { dataView, statusPresentation, effectiveCanMutate, identityView } from './view-models.ts';
test('unknown states preserve raw labels and do not become healthy', () => {
  expect(statusPresentation('NovelState').label).toBe('NovelState');
  expect(statusPresentation('NovelState').tone).toBe('neutral');
});
test('data distinguishes empty from unavailable and retained stale values', () => {
  expect(dataView(undefined, false, new Error('down')).phase).toBe('unavailable');
  expect(dataView([], false, null).phase).toBe('empty');
  expect(dataView([0], false, new Error('down'), 123).phase).toBe('stale');
  expect(dataView([0], false, null, 123).lastSuccessfulFetchAt).toBe(123);
});
test('both live browser transport and Core permission are required', () => {
  expect(effectiveCanMutate(false, { canMutate: true })).toBe(false);
  expect(effectiveCanMutate(true, { canMutate: false })).toBe(false);
  expect(effectiveCanMutate(true, { canMutate: true })).toBe(true);
});
test('identity initials derive only from actual identity', () =>
  expect(identityView({ id: '1', username: 'kyson' }).displayInitials).toBe('KY'));
