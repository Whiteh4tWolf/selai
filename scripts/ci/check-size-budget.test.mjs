import { test } from 'node:test';
import assert from 'node:assert/strict';
import { budgets, evaluate } from './check-size-budget.mjs';

test('separately gates lazy assets, startup JS, native executable, and installer', () => {
  assert.deepEqual(evaluate([{ name: 'diagram.js', kind: 'asset', bytes: budgets.chunk }]).errors, []);
  for (const file of [
    { name: 'index-startup.js', kind: 'asset', bytes: budgets.startup + 1 },
    { name: 'diagram.js', kind: 'asset', bytes: budgets.chunk + 1 },
    { name: 'selai.exe', kind: 'executable', bytes: budgets.executable + 1 },
    { name: 'selai.msi', kind: 'installer', bytes: budgets.installer + 1 },
  ]) assert.ok(evaluate([file]).errors.length > 0);
  const assets = Array.from({ length: 7 }, (_, i) => ({ name: `${i}.js`, kind: 'asset', bytes: 1024 * 1024 }));
  assert.ok(evaluate(assets).errors.some((error) => error.includes('Frontend')));
  assert.ok(evaluate([{ name: 'assets', kind: 'asset', bytes: 0, compressed: budgets.compressed + 1 }]).errors.some((error) => error.includes('Compressed')));
});
