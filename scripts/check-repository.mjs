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
// The generated notices (scripts/generate-third-party-notices.mjs) must exist.
// Reads tracked files only (this runs before npm ci and without Rust): the web
// list must name exactly the production packages of package-lock.json, and
// every crate in the Rust list must still be in Cargo.lock at that version (a
// newly added crate is not detected here; rerun the generator after
// dependency changes).
function noticePackages(path) {
  assert.ok(existsSync(join(root, path)), `${path} is missing; run node scripts/generate-third-party-notices.mjs`);
  const lines = readFileSync(join(root, path), 'utf8').split('\n');
  const start = lines.indexOf('Packages');
  assert.ok(start >= 0 && lines[start + 1] === '--------', `${path}: no package list`);
  const packages = [];
  for (let i = start + 2; i < lines.length && lines[i] !== 'License and notice texts' && !lines[i].startsWith('These packages'); i++) {
    const m = /^(\S+) (\S+)$/.exec(lines[i]);
    if (m) packages.push(`${m[1]} ${m[2]}`);
  }
  assert.ok(packages.length, `${path}: empty package list`);
  return packages;
}
function checkThirdPartyNotices(cargoLock) {
  const rust = noticePackages('licenses/THIRD-PARTY-RUST.txt');
  for (const entry of rust) {
    const [name, version] = entry.split(' ');
    assert.ok(cargoLock.includes(`name = "${name}"\nversion = "${version}"`),
      `licenses/THIRD-PARTY-RUST.txt lists ${entry}, which is not in Cargo.lock; run node scripts/generate-third-party-notices.mjs`);
  }
  const web = new Set(noticePackages('licenses/THIRD-PARTY-WEB.txt'));
  const lock = readJson('package-lock.json');
  const production = Object.entries(lock.packages)
    .filter(([path, info]) => path && !info.dev && !info.devOptional && !info.link && !info.optional)
    .map(([path, info]) => `${path.replace(/^.*node_modules\//, '')} ${info.version}`);
  const stale = [...production.filter(p => !web.has(p)), ...[...web].filter(p => !production.includes(p))];
  assert.deepEqual(stale, [], `licenses/THIRD-PARTY-WEB.txt does not match package-lock.json; run node scripts/generate-third-party-notices.mjs`);
}
// The effect player (tools/effect-player) is a build output, so only its tracked inputs are
// checked here: licenses/effect-player/DEPENDENCIES.json (scripts/generate-effect-player-notices.mjs)
// must account for every package of its packages.lock.json, as shipped (same version and content
// hash) or not shipped, name the project's version, SDK and runtime, and list exactly the notice
// files that are there. Its bundle entry lives in a separate configuration that the build passes
// only when the player was published. scripts/publish-effect-player.ps1 checks the published
// output against the same list.
function checkEffectPlayer(resources) {
  const overlay = readJson('src-tauri/tauri.effect-player.conf.json');
  assert.deepEqual(overlay, {bundle: {resources: {'../resources/native/effect-player/win-x64/Dmb.EffectPlayer.exe': 'native/effect-player/win-x64/Dmb.EffectPlayer.exe'}}},
    'src-tauri/tauri.effect-player.conf.json must hold only the effect player bundle entry');
  assert.ok(!Object.keys(resources).some(source => source.includes('effect-player')),
    'src-tauri/tauri.conf.json must not list the effect player (a build without it would fail)');
  const folder = 'licenses/effect-player';
  const stale = `${folder} does not match tools/effect-player; publish the player and run node scripts/generate-effect-player-notices.mjs`;
  assert.ok(existsSync(join(root, folder, 'DEPENDENCIES.json')), `${folder}/DEPENDENCIES.json is missing; ${stale.split('; ')[1]}`);
  const deps = readJson(`${folder}/DEPENDENCIES.json`);
  const project = readFileSync(join(root, 'tools/effect-player/EffectPlayer.csproj'), 'utf8');
  assert.equal(deps.version, project.match(/<Version>([^<]+)<\/Version>/)?.[1], `${stale} (helper version)`);
  assert.equal(deps.runtime.version, project.match(/<RuntimeFrameworkVersion>([^<]+)<\/RuntimeFrameworkVersion>/)?.[1], `${stale} (runtime version)`);
  assert.equal(deps.sdk, readJson('tools/effect-player/global.json').sdk.version, `${stale} (SDK version)`);
  const lock = new Map();
  for (const section of Object.values(readJson('tools/effect-player/packages.lock.json').dependencies)) {
    for (const [id, info] of Object.entries(section)) lock.set(id, info);
  }
  const accounted = [...deps.packages, ...deps.not_shipped];
  const ids = accounted.map(p => p.id);
  assert.equal(new Set(ids).size, ids.length, `${stale} (a package is listed twice)`);
  assert.deepEqual([...ids].sort(), [...lock.keys()].sort(), stale);
  for (const p of accounted) assert.equal(p.version, lock.get(p.id).resolved, `${stale} (${p.id} version)`);
  for (const p of deps.packages) assert.equal(p.content_hash, lock.get(p.id).contentHash, `${stale} (${p.id} content hash)`);
  const missing = new Set(deps.missing_texts.map(m => m.what));
  const listed = new Set();
  const notice = file => {
    assert.ok(existsSync(join(root, folder, file)) && statSync(join(root, folder, file)).size > 0, `${folder}/${file} is missing or empty`);
    listed.add(file);
  };
  for (const p of [deps.runtime, ...deps.packages]) {
    p.notices.forEach(n => notice(n.file));
    assert.ok(missing.has(`${p.id} ${p.version}`) || p.notices.some(n => /\/(licen[cs]e|copying|dotnet-license)[^/]*$/i.test(n.file)),
      `${folder}: no license text for ${p.id} ${p.version}`);
  }
  for (const c of deps.components) notice(c.notice.file);
  const present = walk(join(root, folder)).map(f => relative(join(root, folder), f).replaceAll('\\', '/')).filter(f => f !== 'DEPENDENCIES.json');
  const unlisted = present.filter(f => !listed.has(f));
  assert.deepEqual(unlisted, [], `${stale} (files not in DEPENDENCIES.json)`);
  if (deps.missing_texts.length) {
    console.log(`Effect player notices: ${deps.missing_texts.length} license text(s) not included yet: ${deps.missing_texts.map(m => `${m.what} (${m.license})`).join('; ')}.`);
  }
}
// The interface-language catalogs (scripts/i18n-check.mjs, plain run: every problem except
// missing entries fails). It reads the sources with the TypeScript package, which this
// script cannot count on (it runs before npm ci); without it the check says so and the UI
// test src/i18n/catalogs.test.ts runs the same check after npm ci.
async function checkLanguages() {
  let ts;
  try { ts = (await import('typescript')).default; }
  catch { console.log('Language catalogs: not checked here (the typescript package is not installed yet); the UI tests check them.'); return; }
  const { checkI18n } = await import('./i18n-check.mjs');
  const result = await checkI18n({ ts });
  if (result.failed) {
    for (const line of result.lines) console.log(line);
    assert.fail('The language catalogs have problems; run node scripts/i18n-check.mjs');
  }
  console.log(result.lines.at(-1));
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
  // The compiled Vindicta reference is optional (scripts/sync-private-assets.mjs copies it in); check its hash only when it is there.
  const baseline = readJson('physics-references/Vindicta-R12-Balanced/baseline.json');
  if (existsSync(join(root, baseline.included_path))) assert.equal(sha256(join(root, baseline.included_path)), baseline.reference_sha256);
  else console.log('Optional data absent: the included Vindicta reference is not part of this build.');
  for (const source of Object.keys(readJson('src-tauri/tauri.conf.json').bundle.resources)) {
    const path = resolve(root, 'src-tauri', source);
    const files = statSync(path).isDirectory() ? walk(path) : [path];
    assert.ok(files.length, `Empty bundle resource: ${source}`);
    for (const file of files) assert.ok(statSync(file).size > 0, `Empty resource: ${file}`);
  }
  checkThirdPartyNotices(lock);
  checkEffectPlayer(readJson('src-tauri/tauri.conf.json').bundle.resources);
  for (const folder of ['src', 'src-tauri', 'resources', 'physics-references', 'licenses', 'tools']) {
    for (const file of walk(join(root, folder)).filter(p => !/[\\/](target|node_modules|bin|obj)[\\/]/.test(p))) {
      assert.ok(statSync(file).size < 100 * 1024 * 1024, `Exceeds GitHub file limit: ${relative(root, file)}`);
    }
  }
  console.log(`Repository preflight passed: ${version}; versions, required resources and reader trust pins match.`);
  return version;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) { checkRepository(); await checkLanguages(); }
