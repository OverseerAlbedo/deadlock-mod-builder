// Copies the private assets that the app needs at run time into the places the
// bundler expects. Today: the compiled Vindicta cloth references, which
// `src-tauri/tauri.conf.json` bundles from `physics-references/`. The private
// assets are a separate repository cloned to `private-assets/` (or named by
// DMB_PRIVATE_ASSETS); without it the build proceeds and the Physics page hides
// the included Vindicta reference. See docs/DEVELOPMENT.md "Private assets".
import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const source = resolve(process.env.DMB_PRIVATE_ASSETS || join(root, 'private-assets'), 'physics-references');
if (!existsSync(source)) {
  console.log(`Private assets not present (${source}); the compiled Vindicta references stay out of this build.`);
  process.exit(0);
}
let copied = 0;
const walk = dir => readdirSync(dir).flatMap(name => {
  const path = join(dir, name);
  return statSync(path).isDirectory() ? walk(path) : [path];
});
for (const file of walk(source).filter(path => path.endsWith('.vmdl_c'))) {
  const target = join(root, 'physics-references', relative(source, file));
  mkdirSync(dirname(target), { recursive: true });
  copyFileSync(file, target);
  copied += 1;
}
console.log(`Private assets: ${copied} compiled reference(s) copied into physics-references/.`);
