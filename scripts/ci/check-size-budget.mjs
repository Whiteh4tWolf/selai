import { readdirSync, readFileSync, statSync } from 'node:fs';
import { resolve, dirname, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const MiB = 1024 * 1024;
export const budgets = Object.freeze({ frontend: 6.5 * MiB, compressed: 2 * MiB, chunk: 1.5 * MiB, startup: 450 * 1024, executable: 40 * MiB, installer: 16 * MiB });

export function evaluate(files) {
  const errors = [];
  const assets = files.filter((file) => file.kind === 'asset');
  const total = assets.reduce((sum, file) => sum + file.bytes, 0);
  const compressed = assets.reduce((sum, file) => sum + (file.compressed ?? 0), 0);
  if (total > budgets.frontend) errors.push('Frontend exceeds 6.5 MiB.');
  if (compressed > budgets.compressed) errors.push('Compressed frontend exceeds 2 MiB.');
  for (const file of files) {
    const limit = file.kind === 'installer' ? budgets.installer : file.kind === 'executable' ? budgets.executable : budgets.chunk;
    if (file.bytes > limit) errors.push(`${file.name} exceeds its ${file.kind} budget.`);
    if (file.kind === 'asset' && /^index-[\w-]+\.js$/.test(file.name) && file.bytes > budgets.startup) errors.push('Startup JS exceeds 450 KiB.');
  }
  return { total, compressed, errors };
}

function walk(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? walk(path) : entry.isFile() ? [path] : [];
  });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const files = walk(resolve(root, 'apps/desktop/dist')).map((path) => ({
    name: path.split(/[\\/]/).pop(), kind: 'asset', bytes: statSync(path).size, compressed: gzipSync(readFileSync(path)).length,
  }));
  if (process.argv.includes('--windows')) {
    const exe = resolve(root, 'target/release/selai-desktop.exe');
    files.push({ name: 'selai-desktop.exe', kind: 'executable', bytes: statSync(exe).size });
    let installers = 0;
    for (const path of walk(resolve(root, 'target/release/bundle'))) {
      if (extname(path) === '.msi' || path.endsWith('-setup.exe')) {
        installers += 1;
        files.push({ name: path.split(/[\\/]/).pop(), kind: 'installer', bytes: statSync(path).size });
      }
    }
    if (!installers) throw new Error('Windows package check requires at least one installer.');
  }
  const result = evaluate(files);
  console.log(JSON.stringify(result, null, 2));
  if (result.errors.length) process.exitCode = 1;
}
