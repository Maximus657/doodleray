import { readdir, readFile } from 'node:fs/promises';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const sourceRoot = fileURLToPath(new URL('../..', import.meta.url));
const allowed = new Set(['platform/tauri/desktop-bridge.ts']);

async function sourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(entries.map(async (entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return sourceFiles(path);
    return /\.(?:ts|tsx)$/.test(entry.name) ? [path] : [];
  }));
  return files.flat();
}

const directCoreImports = [];
for (const file of await sourceFiles(sourceRoot)) {
  const text = await readFile(file, 'utf8');
  if (text.includes('@tauri-apps/api/core')) {
    directCoreImports.push(relative(sourceRoot, file).replaceAll('\\', '/'));
  }
}

const unexpected = directCoreImports.filter((file) => !allowed.has(file));
if (unexpected.length) {
  throw new Error(`Only DesktopBridge may import Tauri core directly: ${unexpected.join(', ')}`);
}

console.log('desktop bridge import boundary tests passed');
