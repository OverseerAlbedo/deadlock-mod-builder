import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, statSync, readdirSync } from 'node:fs';
import { dirname, resolve, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export const readJson = path => JSON.parse(readFileSync(join(root, path), 'utf8'));
export const sha256 = path => createHash('sha256').update(readFileSync(path)).digest('hex');
export function walk(path) {
  return readdirSync(path, {withFileTypes: true}).flatMap(e =>
    e.isDirectory() ? walk(join(path, e.name)) : [join(path, e.name)]);
}
export function checkRepository() {
  const version = readJson('package.json').version;
  assert.equal(readJson('package-lock.json').version, version);
  assert.equal(readJson('package-lock.json').packages[''].version, version);
  assert.equal(readJson('src-tauri/tauri.conf.json').version, version);
  for (const name of ['src-tauri/Cargo.toml', 'src-tauri/crates/deadlock-pipeline/Cargo.toml']) {
    assert.equal(readFileSync(join(root, name), 'utf8').match(/^version = "([^"]+)"/m)?.[1], version, name);
  }
  const lock = readFileSync(join(root, 'src-tauri/Cargo.lock'), 'utf8');
  for (const name of ['deadlock-desktop', 'deadlock-pipeline']) {
    assert.ok(lock.includes(`name = "${name}"\nversion = "${version}"`), `Cargo.lock: ${name}`);
  }
  const manifest = readJson('resources/native/cloth-reader/manifest.json');
  const pins = readFileSync(join(root, 'src-tauri/crates/deadlock-tools/src/cloth_reader.rs'), 'utf8');
  for (const platform of Object.values(manifest.platforms)) {
    for (const [name, hash] of Object.entries(platform.files)) {
      assert.equal(sha256(join(root, 'resources/native/cloth-reader', name)), hash, name);
      assert.ok(pins.includes(hash), `Production trust pin missing for ${name}`);
    }
  }
  // The compiled Vindicta reference is a private asset (scripts/sync-private-assets.mjs copies it in); check its hash only when it is there.
  const baseline = readJson('physics-references/Vindicta-R12-Balanced/baseline.json');
  if (existsSync(join(root, baseline.included_path))) assert.equal(sha256(join(root, baseline.included_path)), baseline.reference_sha256);
  else console.log('Private assets absent: the included Vindicta reference is not part of this build.');
  for (const source of Object.keys(readJson('src-tauri/tauri.conf.json').bundle.resources)) {
    const path = resolve(root, 'src-tauri', source);
    const files = statSync(path).isDirectory() ? walk(path) : [path];
    assert.ok(files.length, `Empty bundle resource: ${source}`);
    for (const file of files) assert.ok(statSync(file).size > 0, `Empty resource: ${file}`);
  }
  for (const folder of ['src', 'src-tauri', 'resources', 'physics-references', 'licenses', 'tools']) {
    for (const file of walk(join(root, folder)).filter(p => !/[\\/](target|node_modules|bin|obj)[\\/]/.test(p))) {
      assert.ok(statSync(file).size < 100 * 1024 * 1024, `Exceeds GitHub file limit: ${relative(root, file)}`);
    }
  }
  console.log(`Repository preflight passed: ${version}; versions, required resources and reader trust pins match.`);
  return version;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) checkRepository();
