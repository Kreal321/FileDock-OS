/// <reference types="node" />
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { RecoveryPoint } from '../src/components/RecoveryPoint.tsx';

function sources(root: string): string[] {
  return readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const path = resolve(root, entry.name);
    return entry.isDirectory()
      ? sources(path)
      : /\.(tsx?|md)$/.test(path) && !path.includes('.test.')
        ? [path]
        : [];
  });
}
test('product copy does not promise lossless service or uninterrupted availability', () => {
  const paths = [...sources(resolve('src')), resolve('../FileDock-OS.md')];
  const promises =
    /(?:zero[- ](?:downtime|data loss)|no data (?:will (?:ever )?be lost|loss guaranteed)|100% (?:uptime|availability)|guaranteed (?:lossless|uninterrupted))/gi;
  for (const path of paths) {
    const text = readFileSync(path, 'utf8');
    for (const line of text.split('\n')) {
      if (/must not|does not|not promise|no promise|not guarantee/i.test(line)) continue;
      expect(line.match(promises), path).toBeNull();
    }
  }
});
test('durable recovery observations render a real timestamp and truthful missing/cached states', () => {
  const html = renderToStaticMarkup(
    createElement(RecoveryPoint, { at: '2026-10-19T12:00:00Z', cached: true }),
  );
  expect(html.toLowerCase()).toContain('datetime="2026-10-19t12:00:00z"');
  expect(html).toContain('Latest durable recovery point');
  expect(html).toContain('Cached observation');
  for (const at of [undefined, '', 'not-a-date']) {
    const missing = renderToStaticMarkup(createElement(RecoveryPoint, { at }));
    expect(missing).toContain('Not reported');
    expect(missing).not.toContain('<time');
  }
});
test('protection status surfaces retain the durable-recovery observation', () => {
  for (const path of [
    'features/dashboard/DashboardPage.tsx',
    'features/recovery/StateProtectionPage.tsx',
    'features/folders/FoldersPage.tsx',
  ]) {
    expect(readFileSync(resolve('src', path), 'utf8'), path).toContain('<RecoveryPoint');
  }
});
