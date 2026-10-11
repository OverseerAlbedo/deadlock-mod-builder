import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, readFileSync, writeFileSync, statSync, existsSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { root, readJson, sha256, walk, checkRepository } from './check-repository.mjs';

function requireWindowsX64(file) {
  const b = readFileSync(file);
  assert.equal(b.toString('ascii', 0, 2), 'MZ', `Not a Windows executable: ${file}`);
  const pe = b.readUInt32LE(0x3c);
  assert.equal(b.toString('ascii', pe, pe + 4), 'PE\0\0');
  assert.equal(b.readUInt16LE(pe + 4), 0x8664, 'Expected Windows x64 executable');
  return b;
}
function applicationImports(file) {
  const b = requireWindowsX64(file);
  const pe = b.readUInt32LE(0x3c), optional = pe + 24;
  assert.equal(b.readUInt16LE(optional), 0x20b, 'Expected PE32+ application');
  const sections = optional + b.readUInt16LE(pe + 20);
  const offset = rva => {
    for (let i = 0; i < b.readUInt16LE(pe + 6); i++) {
      const s = sections + 40*i, start = b.readUInt32LE(s+12);
      if (rva >= start && rva < start + b.readUInt32LE(s+16)) return b.readUInt32LE(s+20) + rva - start;
    }
    throw new Error(`Unmapped import address ${rva}`);
  };
  const rva = b.readUInt32LE(optional + 112 + 8);
  assert.ok(rva, 'Application has no native import table');
  const imports = [];
  for (let entry = offset(rva), n = 0; n < 1024; entry += 20, n++) {
    const name = b.readUInt32LE(entry+12);
    if (!name) return imports;
    const start = offset(name), end = b.indexOf(0, start);
    assert.ok(end > start && end-start < 256, 'Malformed DLL import name');
    imports.push(b.toString('ascii', start, end));
  }
  throw new Error('Import table did not terminate');
}
export function packageWindows(binary, installer, output) {
  const version = checkRepository();
  const imports = applicationImports(binary);
  assert.ok(!imports.some(dll => /^(libgcc|libstdc\+\+|libwinpthread|vcruntime|msvcp)/i.test(dll)),
    `Application has an unbundled compiler/loader runtime: ${imports.join(', ')}`);
  const config = readJson('src-tauri/tauri.conf.json');
  const portable = join(output, 'portable', 'DeadlockModBuilder');
  assert.ok(!existsSync(portable), `Refusing to mix with an existing package: ${portable}`);
  mkdirSync(portable, {recursive: true});
  const records = [];
  const copy = (source, destination) => {
    const target = join(portable, destination);
    mkdirSync(dirname(target), {recursive: true});
    copyFileSync(source, target);
    assert.equal(sha256(target), sha256(source));
    records.push({path: destination.replaceAll('\\', '/'), sha256: sha256(target), bytes: statSync(target).size});
  };
  copy(binary, 'deadlock-mod-builder.exe');
  // Tauri adds this loader to installers automatically on targets that link
  // it dynamically. A portable folder must explicitly carry the same DLL.
  if (imports.some(name => name.toLowerCase() === 'webview2loader.dll')) {
    const loader = join(dirname(binary), 'WebView2Loader.dll');
    requireWindowsX64(loader);
    copy(loader, 'WebView2Loader.dll');
  }
  for (const [source, destination] of Object.entries(config.bundle.resources)) {
    const full = resolve(root, 'src-tauri', source);
    if (statSync(full).isDirectory()) {
      for (const file of walk(full)) copy(file, join(destination, relative(full, file)));
    } else copy(full, destination);
  }
  assert.ok(!existsSync(join(portable, 'native/cloth-reader/linux-x64')));
  requireWindowsX64(join(portable, 'native/cloth-reader/win-x64/Dmb.ClothReader.exe'));
  const manifest = readJson('resources/native/cloth-reader/manifest.json');
  for (const [name, hash] of Object.entries(manifest.platforms['win-x64'].files)) {
    assert.equal(sha256(join(portable, 'native/cloth-reader', name)), hash);
  }
  writeFileSync(join(portable, 'README.txt'), `Deadlock Mod Builder ${version}\n\nExtract this entire folder, then run deadlock-mod-builder.exe.\nKeep native, physics-references and licenses beside the application.\nWindows x64 and Microsoft Edge WebView2 Runtime are required.\nSettings and workspaces use the normal Windows user profile, even in this ZIP build.\nBlender, Deadlock and Source 2 compilation tools are configured separately in Settings.\nSee LICENSE and licenses/ for notices.\n`);
  writeFileSync(join(portable, 'package-manifest.json'), JSON.stringify({version, target:'windows-x64', files:records}, null, 2)+'\n');
  const release = join(output, 'release');
  mkdirSync(release, {recursive:true});
  let setup = null;
  if (installer) {
    // NSIS bootstrap programs can be PE32 even when they install an x64 application.
    assert.equal(readFileSync(installer).toString('ascii', 0, 2), 'MZ');
    assert.ok(basename(installer).includes(version), 'Installer filename/version mismatch');
    setup = join(release, `DeadlockModBuilder-v${version}-windows-x64-setup.exe`);
    copyFileSync(installer, setup);
  }
  const validation = join(output, 'validation');
  mkdirSync(validation, {recursive:true});
  writeFileSync(join(validation, 'package-checks.json'), JSON.stringify({version, payload_files:records.length, application_imports:imports, installer_sha256:setup ? sha256(setup) : null, portable_resources_verified:true, windows_execution_verified:false}, null, 2)+'\n');
  console.log(`Packaged ${records.length} verified payload files into ${portable}`);
  return {portable, setup, version};
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  assert.equal(process.argv.length, 5, 'Usage: node scripts/package-windows.mjs BINARY INSTALLER OUTPUT');
  packageWindows(resolve(process.argv[2]), process.argv[3] === '--no-installer' ? null : resolve(process.argv[3]), resolve(process.argv[4]));
}
