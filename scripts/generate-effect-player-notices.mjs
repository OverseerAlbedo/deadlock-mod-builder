// Rebuilds licenses/effect-player/: the notices of everything inside the effect player's single
// file (tools/effect-player, published by scripts/publish-effect-player.ps1).
//
//   pwsh -NoProfile -File scripts/publish-effect-player.ps1     (with the .NET SDK; see there)
//   node scripts/generate-effect-player-notices.mjs
//
// Offline. Inputs: the list of bundled files the last publish recorded
// (tools/effect-player/obj/Release/net11.0/win-x64/dmb-bundle-contents.txt), packages.lock.json,
// obj/project.assets.json and the restored packages (NUGET_PACKAGES, else ~/.nuget/packages).
// For every package with a file in the output it copies the package's nuspec and the license and
// notice files the package carries. A package that declares a license without carrying its text
// gets the standard text with the copyright lines its ValveResourceFormat entry or its nuspec
// states. Components that the ValveResourceFormat packages name in their THIRD_PARTY_NOTICES.txt
// as built into them get one file each. The standard texts come from files on disk: MIT from the
// .NET runtime pack's LICENSE.TXT, Apache-2.0 and BSD-3-Clause from licenses/THIRD-PARTY-RUST.txt,
// OFL-1.1 from scripts/notice-texts/OFL-1.1.txt.
// The output is sorted and carries no dates or local paths, so the same inputs give the same files.
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const project = join(root, 'tools', 'effect-player');
const out = join(root, 'licenses', 'effect-player');
const RID = 'win-x64';
const RUNTIME_PACK = `Microsoft.NETCore.App.Runtime.${RID}`;
const OWN_FILES = new Set(['Dmb.EffectPlayer.exe', 'Dmb.EffectPlayer.dll', 'Dmb.EffectPlayer.deps.json', 'Dmb.EffectPlayer.runtimeconfig.json']);
const NOTICE_FILE = /^(licen[cs]e|copying|copyright|notice|third[-_]party[-_]notices)([-._ ].*)?$/i;
const LICENSE_FILE = /^(licen[cs]e|copying)([-._ ].*)?$/i;
// Entries of the ValveResourceFormat notice that are packages of their own: their texts come
// with the package. Skia is inside SkiaSharp's native library, whose notices cover it.
const PACKAGED = {
  'Blake3': ['Blake3'], 'K4os.Compression.LZ4': ['K4os.Compression.LZ4'], 'KeyValues2': ['KeyValues2'],
  'NLayer': ['NLayer'], 'OpenTK': ['OpenTK.'], 'SharpGLTF': ['SharpGLTF.'], 'SkiaSharp': ['SkiaSharp'],
  'Skia': ['SkiaSharp.NativeAssets.Win32'], 'TinyBCSharp': ['TinyBCSharp'], 'ValveKeyValue': ['ValveKeyValue'],
  'ValvePak': ['ValvePak'], 'Vortice.SpirvCross': ['Vortice.'], '.NET Runtime and Libraries': ['System.', RUNTIME_PACK],
};
// Components inside a native library of another package (not inside ValveResourceFormat).
const INSIDE = { 'SPIRV-Cross': 'Vortice.SpirvCross (spirv-cross.dll)' };
// Packages whose nuspec names a license file instead of an expression: the license that file is.
const FILE_LICENSES = { 'OpenTK.redist.glfw': 'Zlib' };

const read = path => readFileSync(path, 'utf8');
const normalize = text => text.replace(/^﻿/, '').replace(/\r\n?/g, '\n').split('\n').map(l => l.replace(/[ \t]+$/, '')).join('\n').trim() + '\n';
const byText = (a, b) => a.localeCompare(b, 'en', { numeric: true });
const decode = s => s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&apos;/g, "'").replace(/&amp;/g, '&').trim();
const slug = name => name.replace(/\(.*?\)/g, '').trim().replace(/[^A-Za-z0-9.+-]+/g, '-').replace(/^-+|-+$/g, '');

function packagesFolder() {
  const folder = process.env.NUGET_PACKAGES || join(homedir(), '.nuget', 'packages');
  if (!existsSync(folder)) throw new Error(`No restored packages at ${folder}; publish the effect player first (NUGET_PACKAGES names the folder).`);
  return folder;
}
function nuspec(dir, id) {
  const file = readdirSync(dir).find(n => n.toLowerCase() === `${id.toLowerCase()}.nuspec`);
  if (!file) throw new Error(`${id}: no nuspec in its package folder`);
  const text = read(join(dir, file));
  const tag = name => { const m = new RegExp(`<${name}(\\s[^>]*)?>([\\s\\S]*?)</${name}>`).exec(text); return m ? decode(m[2]) : null; };
  const license = /<license\s+type="(\w+)"\s*>([^<]*)<\/license>/.exec(text);
  const repo = /<repository\s([^>]*)\/?>/.exec(text);
  const attr = (s, name) => (new RegExp(`${name}="([^"]*)"`).exec(s || '') || [])[1] || null;
  return {
    file, authors: tag('authors'), copyright: tag('copyright'), projectUrl: tag('projectUrl'), licenseUrl: tag('licenseUrl'),
    license: license ? { type: license[1], value: decode(license[2]) } : null,
    repository: repo ? Object.fromEntries(Object.entries({ url: attr(repo[1], 'url'), commit: attr(repo[1], 'commit') }).filter(([, v]) => v)) : null,
  };
}

// The ValveResourceFormat notice: one block per project, "  License:", copyright lines, links.
function vrfComponents(text) {
  const lines = normalize(text).split('\n');
  const start = lines.findIndex((l, i) => /^\S/.test(l) && lines[i + 1]?.startsWith('  License:'));
  const end = lines.indexOf('Licenses');
  const blocks = [];
  let block = null;
  for (const line of lines.slice(start, end)) {
    if (!line.trim()) { block = null; continue; }
    if (!line.startsWith(' ')) { block = { title: line, name: line.replace(/\s*\((GUI|CLI) only\)\s*$/, ''), only: /\((GUI|CLI) only\)/.test(line), license: '', copyright: [], urls: [] }; blocks.push(block); continue; }
    const value = line.trim();
    if (value.startsWith('License:')) block.license = value.slice(8).trim();
    else if (/^https?:\/\//.test(value)) block.urls.push(value);
    else block.copyright.push(value);
  }
  return blocks;
}

// Standard texts, from files on disk.
function standardTexts(runtimeDir) {
  const dotnet = normalize(read(join(runtimeDir, 'LICENSE.TXT')));
  const mit = dotnet.slice(dotnet.indexOf('Permission is hereby granted'));
  if (!mit.startsWith('Permission is hereby granted')) throw new Error('No MIT text in the runtime pack LICENSE.TXT');
  const rust = normalize(read(join(root, 'licenses', 'THIRD-PARTY-RUST.txt')));
  // Text blocks, most used first (the most common spelling of a license text).
  const parts = rust.split(/\n-{78}\nText \d+ \([^)]*\)\nUsed by: ([^\n]*)\n-{78}\n\n/);
  const blocks = [];
  for (let i = 1; i + 1 < parts.length; i += 2) blocks.push({ users: parts[i].split(', ').length, text: parts[i + 1].replace(/\n+$/, '') });
  blocks.sort((a, b) => b.users - a.users || a.text.localeCompare(b.text, 'en'));
  return pickTexts(dotnet, mit, blocks.map(b => b.text));
}
function pickTexts(dotnet, mit, blocks) {
  const apacheBlock = blocks.find(b => b.startsWith('Apache License') && b.includes('Version 2.0, January 2004') && b.includes('END OF TERMS AND CONDITIONS'));
  if (!apacheBlock) throw new Error('No Apache-2.0 text in licenses/THIRD-PARTY-RUST.txt');
  const apache = apacheBlock.slice(0, apacheBlock.indexOf('END OF TERMS AND CONDITIONS') + 'END OF TERMS AND CONDITIONS'.length) + '\n';
  const bsd3Block = blocks.find(b => b.includes('3. Neither the name of the copyright holder nor the names of its contributors may be used')
    && !b.includes('\n4.') && b.includes('Redistribution and use in source and binary forms'));
  if (!bsd3Block) throw new Error('No BSD-3-Clause text in licenses/THIRD-PARTY-RUST.txt');
  const bsd3 = bsd3Block.slice(bsd3Block.indexOf('Redistribution and use in source and binary forms')).trim() + '\n';
  // OFL-1.1 (fonts built into ValveResourceFormat): no package carries it, so the text is kept
  // in scripts/notice-texts/ (the SPDX text as Blender ships it in license/spdx/OFL-1.1.txt).
  const ofl = normalize(read(join(root, 'scripts', 'notice-texts', 'OFL-1.1.txt')));
  return {
    'OFL-1.1': { text: ofl, origin: 'the SIL Open Font License 1.1 text, from scripts/notice-texts/OFL-1.1.txt' },
    'MIT': { text: mit, origin: 'the standard MIT text, from the .NET runtime pack LICENSE.TXT' },
    'Apache-2.0': { text: apache, origin: 'the standard Apache License 2.0 text, from licenses/THIRD-PARTY-RUST.txt' },
    'BSD-3-Clause': { text: bsd3, origin: 'the standard BSD-3-Clause text, from licenses/THIRD-PARTY-RUST.txt' },
  };
}
// The text a license expression is used under ("A OR B": the first one with a text here).
function chooseLicense(expression, texts) {
  for (const option of String(expression).split(/\s+OR\s+/)) if (texts[option.trim()]) return option.trim();
  return null;
}

function lockPackages() {
  const lock = JSON.parse(read(join(project, 'packages.lock.json')));
  const all = new Map();
  for (const section of Object.values(lock.dependencies)) {
    for (const [id, info] of Object.entries(section)) all.set(id, { id, version: info.resolved, content_hash: info.contentHash });
  }
  return all;
}

function main() {
  const contentsFile = join(project, 'obj', 'Release', 'net11.0', RID, 'dmb-bundle-contents.txt');
  if (!existsSync(contentsFile)) throw new Error('No bundle contents; publish the effect player first (scripts/publish-effect-player.ps1).');
  const bundled = new Map();
  for (const line of read(contentsFile).split(/\r?\n/).filter(Boolean)) {
    const [path, id, version] = line.split('|');
    if (!id) { if (!OWN_FILES.has(path)) throw new Error(`Bundled file from no package: ${path}`); continue; }
    const key = `${id} ${version}`;
    if (!bundled.has(key)) bundled.set(key, { id, version, files: [] });
    bundled.get(key).files.push(path);
  }
  const lock = lockPackages();
  const assets = JSON.parse(read(join(project, 'obj', 'project.assets.json')));
  const csproj = read(join(project, 'EffectPlayer.csproj'));
  const helperVersion = /<Version>([^<]+)<\/Version>/.exec(csproj)[1];
  const sdk = JSON.parse(read(join(project, 'global.json'))).sdk.version;
  const folder = packagesFolder();
  const pkgDir = (id, version) => join(folder, id.toLowerCase(), version.toLowerCase());

  const runtime = [...bundled.values()].find(p => p.id === RUNTIME_PACK);
  if (!runtime) throw new Error('The bundle has no runtime pack files; the effect player must be self-contained.');
  const runtimeDir = pkgDir(runtime.id, runtime.version);
  const texts = standardTexts(runtimeDir);
  const vrfPackage = lock.get('ValveResourceFormat');
  const vrfNotice = read(join(pkgDir('ValveResourceFormat', vrfPackage.version), 'THIRD_PARTY_NOTICES.txt'));
  const components = vrfComponents(vrfNotice);
  const componentFor = id => components.find(c => (PACKAGED[c.name] || []).some(p => p.endsWith('.') ? id.startsWith(p) : id === p) && !c.name.startsWith('.NET') && c.name !== 'Skia');

  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  const write = (relative, text) => { mkdirSync(dirname(join(out, relative)), { recursive: true }); writeFileSync(join(out, relative), text); return relative.replaceAll('\\', '/'); };
  const copy = (relative, source) => { mkdirSync(dirname(join(out, relative)), { recursive: true }); copyFileSync(source, join(out, relative)); return relative.replaceAll('\\', '/'); };
  const missing = [];

  // The .NET runtime (self-contained): its license and third-party notices.
  const runtimeSpec = nuspec(runtimeDir, runtime.id);
  const runtimeEntry = {
    id: runtime.id, version: runtime.version, license: runtimeSpec.license?.value, copyright: runtimeSpec.copyright,
    repository: runtimeSpec.repository, bundled_files: runtime.files.length,
    notices: ['LICENSE.TXT', 'THIRD-PARTY-NOTICES.TXT'].map(n => ({ file: copy(`dotnet-runtime-${RID}/${n}`, join(runtimeDir, n)), origin: `the runtime pack ${runtime.id} ${runtime.version}` })),
  };

  const packages = [];
  for (const p of [...bundled.values()].filter(p => p.id !== RUNTIME_PACK).sort((a, b) => byText(a.id, b.id))) {
    const locked = lock.get(p.id);
    if (!locked || locked.version !== p.version) throw new Error(`${p.id} ${p.version} is bundled but not in packages.lock.json at that version`);
    const dir = pkgDir(p.id, p.version);
    const spec = nuspec(dir, p.id);
    const vrf = componentFor(p.id);
    const notices = [{ file: copy(`${p.id}/${spec.file.toLowerCase()}`, join(dir, spec.file)), origin: 'the package (its metadata)' }];
    const carried = readdirSync(dir).filter(n => NOTICE_FILE.test(n) && statSync(join(dir, n)).isFile()).sort(byText);
    for (const name of carried) notices.push({ file: copy(`${p.id}/${name}`, join(dir, name)), origin: 'the package' });
    let license = spec.license?.type === 'expression' ? spec.license.value : (vrf?.license || null);
    if (spec.license?.type === 'file') {
      if (!FILE_LICENSES[p.id]) throw new Error(`${p.id}: its license is the file ${spec.license.value}; add what it is to FILE_LICENSES`);
      license = FILE_LICENSES[p.id];
    }
    if (!carried.some(n => LICENSE_FILE.test(n))) {
      if (spec.repository?.url === 'https://github.com/dotnet/dotnet') {
        // .NET libraries: the .NET license, which the runtime pack carries.
        notices.push({ file: copy(`${p.id}/DOTNET-LICENSE.TXT`, join(runtimeDir, 'LICENSE.TXT')), origin: `the runtime pack ${runtime.id} ${runtime.version} (the same repository)` });
      } else {
        const chosen = chooseLicense(license, texts);
        const lines = vrf?.copyright.length ? vrf.copyright : (spec.copyright && /copyright|\(c\)|©/i.test(spec.copyright) ? [spec.copyright] : []);
        if (!chosen || !lines.length) {
          missing.push({ what: `${p.id} ${p.version}`, license: license || spec.licenseUrl, need: !chosen ? 'the license text' : 'a copyright line' });
        } else {
          const from = vrf?.copyright.length ? `the copyright lines of the ${vrf.name} entry in ValveResourceFormat ${vrfPackage.version} THIRD_PARTY_NOTICES.txt` : 'the copyright line of the package nuspec';
          notices.push({ file: write(`${p.id}/LICENSE-${chosen}.txt`, `${lines.join('\n')}\n\n${texts[chosen].text}`), origin: `${texts[chosen].origin}, with ${from}` });
        }
      }
    }
    packages.push({
      id: p.id, version: p.version, content_hash: locked.content_hash, license: license || spec.licenseUrl,
      copyright: spec.copyright, repository: spec.repository || (spec.projectUrl ? { url: spec.projectUrl } : null),
      bundled_files: p.files.sort(byText), notices,
    });
  }

  // Components built into ValveResourceFormat (or a native library) that are not packages here.
  const shippedIds = new Set(packages.map(p => p.id));
  const embedded = [];
  for (const c of components.filter(c => !c.only && !PACKAGED[c.name]).sort((a, b) => byText(a.name, b.name))) {
    const chosen = chooseLicense(c.license, texts);
    const usedUnder = chosen && chosen !== c.license ? `Used here under: ${chosen}\n` : '';
    const header = `${c.name}\nLicense: ${c.license}\n${usedUnder}${c.urls.join('\n')}\n\n${c.copyright.join('\n')}\n`;
    const inside = INSIDE[c.name] || `ValveResourceFormat ${vrfPackage.version} / ValveResourceFormat.Renderer ${lock.get('ValveResourceFormat.Renderer').version}`;
    if (!chosen) missing.push({ what: `${c.name} (inside ${inside})`, license: c.license, need: 'the license text' });
    const file = write(`components/${slug(c.name)}.txt`, chosen ? `${header}\n${texts[chosen].text}` : header);
    embedded.push({ name: c.name, license: c.license, used_under: chosen, copyright: c.copyright, url: c.urls[0] || null, inside,
      notice: { file, origin: `the ${c.name} entry of ValveResourceFormat ${vrfPackage.version} THIRD_PARTY_NOTICES.txt${chosen ? `, with ${texts[chosen].origin}` : ''}` } });
  }

  const notShipped = [];
  for (const p of [...lock.values()].filter(p => !shippedIds.has(p.id)).sort((a, b) => byText(a.id, b.id))) {
    const target = assets.targets[`net11.0/${RID}`][`${p.id}/${p.version}`] || {};
    const real = group => Object.keys(target[group] || {}).some(f => !f.endsWith('/_._'));
    const reason = !real('runtime') && !real('native') && !real('runtimeTargets')
      ? (real('compile') ? 'compile-time only' : (target.build ? 'a build tool (it adds build steps; none of its files is in the output)' : `no files for ${RID} (assets for other platforms only)`))
      : 'removed by trimming (no file of it is in the output)';
    notShipped.push({ id: p.id, version: p.version, reason });
  }

  const dependencies = {
    helper: 'Dmb.EffectPlayer', version: helperVersion, sdk, rid: RID,
    shipped_as: `native/effect-player/${RID}/Dmb.EffectPlayer.exe (self-contained, trimmed, compressed single file)`,
    generated_by: 'scripts/generate-effect-player-notices.mjs; do not edit by hand',
    runtime: runtimeEntry, packages, components: embedded, not_shipped: notShipped, missing_texts: missing,
  };
  write('DEPENDENCIES.json', JSON.stringify(dependencies, null, 2) + '\n');
  console.log(`Wrote licenses/effect-player: ${packages.length} packages, the runtime, ${embedded.length} built-in components, ${notShipped.length} locked packages not shipped.`);
  for (const m of missing) console.log(`Missing: ${m.need} for ${m.what} (${m.license}).`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
