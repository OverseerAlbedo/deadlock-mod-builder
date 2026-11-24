// Writes the notices of the third-party packages that ship inside the Windows
// app: licenses/THIRD-PARTY-RUST.txt (crates compiled into the executable,
// from `cargo tree` and `cargo metadata`) and licenses/THIRD-PARTY-WEB.txt (production npm
// packages bundled into the interface, from package-lock.json and
// node_modules). Offline; needs the crates in the local Cargo registry (any
// earlier build or `cargo fetch`) and `npm ci`. Run it again after changing
// dependencies. The output is sorted and carries no dates or local paths, so
// an unchanged dependency set gives byte-identical files. A crate published
// without license files of its own takes them from
// scripts/notice-texts/crates/<name>-<version>/ when that folder exists (its
// ORIGIN.txt says where the texts come from).
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const TARGET = 'x86_64-pc-windows-msvc';
const FEATURES = 'desktop'; // as in src-tauri/tauri.conf.json build.features
const NOTICE_FILE = /^(licen[cs]e|copying|copyright|notice|unlicense|patents)([-._ ].*)?$/i;

const normalize = text => text.replace(/^﻿/, '').replace(/\r\n?/g, '\n')
  .split('\n').map(line => line.replace(/[ \t]+$/, '')).join('\n').trim();
const byName = (a, b) => a.name.localeCompare(b.name, 'en') || a.version.localeCompare(b.version, 'en', { numeric: true });

function noticeFiles(dir, extra = []) {
  const names = existsSync(dir) ? readdirSync(dir).filter(name => NOTICE_FILE.test(name) && statSync(join(dir, name)).isFile()) : [];
  for (const file of extra) if (file && existsSync(join(dir, file)) && !names.includes(file)) names.push(file);
  return names.sort((a, b) => a.localeCompare(b, 'en')).map(name => ({ name, text: normalize(readFileSync(join(dir, name), 'utf8')) }))
    .filter(file => file.text);
}

function render(title, intro, packages) {
  packages.sort(byName);
  const texts = new Map();
  const missing = [];
  for (const p of packages) {
    if (!p.files.length) missing.push(p);
    for (const file of p.files) {
      // Texts that differ only in line wrapping or spacing are printed once.
      const key = file.text.replace(/\s+/g, ' ');
      if (!texts.has(key)) texts.set(key, { text: file.text, names: new Set(), users: [] });
      const entry = texts.get(key);
      if (file.text.localeCompare(entry.text, 'en') < 0) entry.text = file.text; // the same choice in every run
      entry.names.add(file.name);
      entry.users.push(`${p.name} ${p.version}`);
    }
  }
  const out = [title, '='.repeat(title.length), '', ...intro, '',
    `${packages.length} packages. Each is listed with its version, its license as the package declares it (SPDX`,
    'expression) and its source repository. The license and notice files the packages carry follow the list;',
    'a text used by several packages is printed once, followed by the packages that carry it.', '',
    'Packages', '--------', ''];
  for (const p of packages) out.push(`${p.name} ${p.version}`, `  License: ${p.license || 'not declared'}`, `  Source: ${p.repository || 'not declared'}`, '');
  if (missing.length) {
    out.push('These packages carry no license file of their own; their declared license applies:', '');
    for (const p of missing) out.push(`  ${p.name} ${p.version} (${p.license || 'not declared'})`);
    out.push('');
  }
  out.push('License and notice texts', '------------------------', '');
  const groups = [...texts.values()].map(g => ({ ...g, users: [...new Set(g.users)].sort((a, b) => a.localeCompare(b, 'en', { numeric: true })) }))
    .sort((a, b) => a.users[0].localeCompare(b.users[0], 'en', { numeric: true }) || a.text.localeCompare(b.text, 'en'));
  groups.forEach((g, i) => {
    out.push('-'.repeat(78), `Text ${i + 1} (${[...g.names].sort().join(', ')})`, `Used by: ${g.users.join(', ')}`, '-'.repeat(78), '', g.text, '');
  });
  return out.join('\n').replace(/\n+$/, '\n');
}

/** A crate's own license files, else the ones kept for it in scripts/notice-texts/crates/. */
function cargoNotices(p) {
  const own = noticeFiles(dirname(p.manifest_path), [p.license_file]);
  return own.length ? own : noticeFiles(join(root, 'scripts', 'notice-texts', 'crates', `${p.name}-${p.version}`));
}

function rustPackages() {
  const cargo = process.env.CARGO || 'cargo';
  const manifest = join(root, 'src-tauri', 'Cargo.toml');
  const run = args => execFileSync(cargo, [...args, '--locked', '--offline', '--manifest-path', manifest],
    { encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });
  // The set comes from `cargo tree`, which resolves features the way the app
  // build does (`cargo metadata` unifies features across the whole workspace,
  // dev-dependencies included, and would list crates the app never compiles).
  // Normal and build dependencies only: dev-dependencies never reach the app.
  const tree = run(['tree', '-p', 'deadlock-desktop', '--features', FEATURES, '-e', 'normal,build',
    '--target', TARGET, '--prefix', 'none', '--format', '{p}']);
  const wanted = new Set(tree.split(/\r?\n/).map(line => /^(\S+) v(\S+)/.exec(line)).filter(Boolean)
    .map(m => `${m[1]} ${m[2]}`));
  // Package details (repository, the crate's folder in the registry) from metadata.
  const meta = JSON.parse(run(['metadata', '--format-version', '1', '--features', FEATURES, '--filter-platform', TARGET]));
  const members = new Set(meta.workspace_members);
  const found = new Map();
  for (const p of meta.packages) {
    const key = `${p.name} ${p.version}`;
    if (wanted.has(key) && !members.has(p.id)) found.set(key, p);
  }
  const workspace = new Set(meta.packages.filter(p => members.has(p.id)).map(p => `${p.name} ${p.version}`));
  const unknown = [...wanted].filter(key => !found.has(key) && !workspace.has(key));
  if (unknown.length) throw new Error(`cargo tree lists crates cargo metadata does not: ${unknown.join(', ')}`);
  return [...found.values()].map(p => {
    return { name: p.name, version: p.version, license: p.license || (p.license_file ? `see ${p.license_file}` : ''),
      repository: p.repository || p.homepage || `https://crates.io/crates/${p.name}/${p.version}`,
      files: cargoNotices(p) };
  });
}

function webPackages() {
  const lock = JSON.parse(readFileSync(join(root, 'package-lock.json'), 'utf8'));
  const out = [];
  for (const [path, info] of Object.entries(lock.packages)) {
    // The same rule as scripts/check-repository.mjs: production packages, by
    // the name of their lock path; optional ones (platform builds) left out.
    if (path === '' || info.dev || info.devOptional || info.link || info.optional) continue;
    const dir = join(root, path);
    const manifest = join(dir, 'package.json');
    if (!existsSync(manifest)) throw new Error(`${path} is missing; run npm ci first`);
    const pkg = JSON.parse(readFileSync(manifest, 'utf8'));
    const repo = typeof pkg.repository === 'string' ? pkg.repository : pkg.repository?.url;
    const license = info.license || pkg.license || (Array.isArray(pkg.licenses) ? pkg.licenses.map(l => l.type || l).join(' OR ') : '');
    out.push({ name: path.replace(/^.*node_modules\//, ''), version: info.version, license,
      repository: (repo || pkg.homepage || `https://www.npmjs.com/package/${pkg.name}`).replace(/^git\+/, ''),
      files: noticeFiles(dir) });
  }
  return out;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const rust = rustPackages();
  writeFileSync(join(root, 'licenses', 'THIRD-PARTY-RUST.txt'), render('Third-party Rust crates in Deadlock Mod Builder',
    [`The crates compiled into the Windows app (${TARGET}, feature "${FEATURES}"): normal and build`,
      'dependencies of the application, from src-tauri/Cargo.lock. Generated by',
      'scripts/generate-third-party-notices.mjs; do not edit by hand.'], rust));
  const web = webPackages();
  writeFileSync(join(root, 'licenses', 'THIRD-PARTY-WEB.txt'), render('Third-party web packages in Deadlock Mod Builder',
    ['The production npm packages bundled into the application interface, from package-lock.json.',
      'Generated by scripts/generate-third-party-notices.mjs; do not edit by hand.'], web));
  console.log(`Wrote licenses/THIRD-PARTY-RUST.txt (${rust.length} crates) and licenses/THIRD-PARTY-WEB.txt (${web.length} packages).`);
}
