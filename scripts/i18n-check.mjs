// The interface-language checker. Reads every source file under src/ (tests excluded) for
// the calls of src/i18n (t, tc, tp, tpc, m, mc, <T text>) and compares what they show with
// the catalogs in src/i18n/locales/<language>/<area>.json.
//
//   node scripts/i18n-check.mjs              everything, by file and line; fails on every problem except missing entries
//   node scripts/i18n-check.mjs --release    also fails on missing entries
//   node scripts/i18n-check.mjs --update     adds missing entries (empty) to the area files
//   node scripts/i18n-check.mjs --keys       prints the keys found, one per line
//   node scripts/i18n-check.mjs --dynamic    also lists the calls that translate a value at run time
//
// Keys: the English text itself; `context|text` for tc / mc / <T context>; `#|other` for tp
// and `#context|other` for tpc (the English `other` form). Each source file names its area in
// a comment `// i18n-area: <name>`; its keys belong in `<area>.json` of every language.
//
// It also reports translated text flowing where it must not go (as far as a static reading
// can tell): a translating call inside an argument of run, operate, fail, invoke,
// updateProject (also as a member: props.fail), an api.* call or localStorage/sessionStorage.setItem; as an operand of
// === / !== / == / !=; as a React key; as a computed object key.
import { readdirSync, readFileSync, writeFileSync, statSync, existsSync } from 'node:fs';
import { join, relative, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const src = join(root, 'src');
const localeRoot = join(src, 'i18n', 'locales');
const TRANSLATE = new Set(['t', 'tc', 'tp', 'tpc']);
const MARK = new Set(['m', 'mc']);
const ALL = new Set([...TRANSLATE, ...MARK]);
const FORBIDDEN_CALLS = new Set(['run', 'operate', 'fail', 'invoke', 'updateProject']);
const PLURAL_FORMS = new Set(['zero', 'one', 'two', 'few', 'many', 'other']);

function sourceFiles(dir) {
  return readdirSync(dir).flatMap(name => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === 'locales' ? [] : sourceFiles(path);
    return /\.(ts|tsx)$/.test(name) && !/\.test\.|\.d\.ts$/.test(name) ? [path] : [];
  });
}
const isI18nModule = spec => /(^|\/)i18n(\/index)?$/.test(spec);

/** Reads every source file; returns keys, areas and the problems found in the code. */
export async function readSources(ts) {
  const keys = new Map();     // key -> {kind, english, area:Set, where:[]}
  const problems = [];        // {where, text}
  const dynamic = [];
  const areas = new Map();    // area -> Set(key)
  const files = sourceFiles(src);
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    const rel = relative(root, file).replaceAll('\\', '/');
    const inI18n = rel.startsWith('src/i18n/');
    const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, file.endsWith('x') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
    const at = node => `${rel}:${source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1}`;
    const area = /\/\/\s*i18n-area:\s*([\w-]+)/.exec(text)?.[1];
    // Names imported from src/i18n (or declared there).
    const imported = new Set();
    for (const statement of source.statements) {
      if (ts.isImportDeclaration(statement) && ts.isStringLiteral(statement.moduleSpecifier) && (isI18nModule(statement.moduleSpecifier.text) || (inI18n && statement.moduleSpecifier.text === './index'))) {
        const named = statement.importClause?.namedBindings;
        if (named && ts.isNamedImports(named)) for (const element of named.elements) imported.add(element.name.text);
      }
    }
    if (inI18n && rel === 'src/i18n/index.ts') for (const name of ALL) imported.add(name);
    const literal = node => node && (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) ? node.text : undefined;
    const declares = (nameNode, name) => {
      if (!nameNode) return false;
      if (ts.isIdentifier(nameNode)) return nameNode.text === name;
      if (ts.isObjectBindingPattern(nameNode) || ts.isArrayBindingPattern(nameNode)) return nameNode.elements.some(element => !ts.isOmittedExpression(element) && declares(element.name, name));
      return false;
    };
    const isUseT = init => init && ts.isCallExpression(init) && ts.isIdentifier(init.expression) && init.expression.text === 'useT';
    /** 'i18n' when `name` at `node` is the module's function, 'other' when a nearer binding hides it. */
    const binding = (name, node) => {
      for (let scope = node.parent; scope; scope = scope.parent) {
        if (ts.isFunctionLike(scope) && scope.parameters?.some(parameter => declares(parameter.name, name))) return 'other';
        if ((ts.isFunctionDeclaration(scope) || ts.isFunctionExpression(scope)) && scope.name?.text === name && scope !== node) return 'other';
        if (ts.isCatchClause(scope) && declares(scope.variableDeclaration?.name, name)) return 'other';
        if ((ts.isForStatement(scope) || ts.isForOfStatement(scope) || ts.isForInStatement(scope)) && scope.initializer && ts.isVariableDeclarationList(scope.initializer) && scope.initializer.declarations.some(d => declares(d.name, name))) return 'other';
        const statements = ts.isBlock(scope) || ts.isSourceFile(scope) || ts.isCaseClause(scope) || ts.isDefaultClause(scope) || ts.isModuleBlock(scope) ? scope.statements : undefined;
        if (!statements) continue;
        for (const statement of statements) {
          if (ts.isVariableStatement(statement)) {
            for (const declaration of statement.declarationList.declarations) if (declares(declaration.name, name)) return isUseT(declaration.initializer) ? 'i18n' : (inI18n && ts.isSourceFile(scope) ? 'i18n' : 'other');
          } else if (ts.isFunctionDeclaration(statement) && statement.name?.text === name) return inI18n ? 'i18n' : 'other';
          else if (ts.isClassDeclaration(statement) && statement.name?.text === name) return 'other';
        }
        if (ts.isSourceFile(scope)) return imported.has(name) ? 'i18n' : 'none';
      }
      return 'none';
    };
    const add = (key, kind, english, node) => {
      const where = at(node);
      if (!area) { problems.push({ where, text: `no area: the file has no "// i18n-area: <name>" comment` }); return; }
      const known = keys.get(key);
      if (known) {
        known.where.push(where); known.area.add(area); known.firstIn[area] ??= where;
        const plainish = k => k === 'text' || k === 'rich';
        if (plainish(known.kind) !== plainish(kind)) problems.push({ where, text: `same key, plural and plain: ${short(key)} (also ${known.where[0]})` });
        else if (kind === 'plural' && JSON.stringify(known.english) !== JSON.stringify(english)) problems.push({ where, text: `same key, other English forms: ${short(key)} (also ${known.where[0]})` });
      } else keys.set(key, { kind, english, area: new Set([area]), where: [where], firstIn: { [area]: where } });
      if (!areas.has(area)) areas.set(area, new Set());
      areas.get(area).add(key);
    };
    const contextOk = (context, node) => {
      if (context === undefined) { problems.push({ where: at(node), text: 'context is not a string literal' }); return false; }
      if (!context || context.includes('|') || context.startsWith('#')) { problems.push({ where: at(node), text: `context "${context}" must be non-empty, without "|" and not start with "#"` }); return false; }
      return true;
    };
    const plainKey = (value, node) => { if (value.includes('|')) problems.push({ where: at(node), text: `plain text with "|" reads as a context key; use tc(): ${short(value)}` }); return value; };
    const formsOf = (node, call) => {
      if (!node || !ts.isObjectLiteralExpression(node)) { problems.push({ where: at(call), text: `${call.expression.getText(source)}() without a literal forms object` }); return undefined; }
      const forms = {};
      for (const property of node.properties) {
        if (!ts.isPropertyAssignment(property)) { problems.push({ where: at(call), text: 'plural forms must be plain properties' }); return undefined; }
        const name = property.name.getText(source).replace(/^['"]|['"]$/g, '');
        const value = literal(property.initializer);
        if (value === undefined) { problems.push({ where: at(call), text: `plural form "${name}" is not a string literal` }); return undefined; }
        forms[name] = value;
      }
      if (!forms.other) { problems.push({ where: at(call), text: 'plural forms without "other"' }); return undefined; }
      return forms;
    };
    /** Translated text flowing where it must not go. */
    const flow = call => {
      let child = call;
      for (let node = call.parent; node; child = node, node = node.parent) {
        if (ts.isFunctionLike(node)) return;
        if (ts.isBinaryExpression(node) && [ts.SyntaxKind.EqualsEqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsEqualsToken, ts.SyntaxKind.EqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsToken].includes(node.operatorToken.kind)) {
          problems.push({ where: at(call), text: `translated text compared (${node.operatorToken.getText(source)}): compare the English value` }); return;
        }
        if (ts.isJsxAttribute(node) && node.name.getText(source) === 'key') { problems.push({ where: at(call), text: 'translated text as a React key' }); return; }
        if (ts.isComputedPropertyName(node)) { problems.push({ where: at(call), text: 'translated text as an object key' }); return; }
        if (ts.isCallExpression(node) && node.arguments.includes(child)) {
          const callee = node.expression;
          const name = ts.isIdentifier(callee) ? callee.text : ts.isPropertyAccessExpression(callee) ? `${callee.expression.getText(source)}.${callee.name.text}` : '';
          const target = ts.isIdentifier(callee) ? callee.text : ts.isPropertyAccessExpression(callee) ? callee.name.text : '';
          if (FORBIDDEN_CALLS.has(target) || /^api\./.test(name) || /^(window\.)?(localStorage|sessionStorage)\.setItem$/.test(name)) {
            problems.push({ where: at(call), text: `translated text passed to ${name}(): pass the English value` }); return;
          }
        }
      }
    };
    const visit = node => {
      if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && ALL.has(node.expression.text) && binding(node.expression.text, node.expression) === 'i18n') {
        const name = node.expression.text, args = node.arguments;
        const withContext = name === 'tc' || name === 'mc' || name === 'tpc';
        const context = withContext ? literal(args[0]) : undefined;
        const textArg = name === 'tp' ? undefined : name === 'tpc' ? undefined : args[withContext ? 1 : 0];
        if (name === 'tp' || name === 'tpc') {
          if (name === 'tpc' && !contextOk(context, node)) { /* reported */ }
          else { const forms = formsOf(args[name === 'tp' ? 1 : 2], node); if (forms) add(`#${context ?? ''}|${forms.other}`, 'plural', forms, node); }
        } else {
          const value = literal(textArg);
          if (value !== undefined) {
            if (!withContext) add(plainKey(value, node), 'text', value, node);
            else if (contextOk(context, node)) add(`${context}|${value}`, 'text', value, node);
          } else if (textArg && ts.isTemplateExpression(textArg)) problems.push({ where: at(node), text: `built from template pieces: ${short(textArg.getText(source))}` });
          else if (textArg && ts.isBinaryExpression(textArg) && textArg.operatorToken.kind === ts.SyntaxKind.PlusToken) problems.push({ where: at(node), text: `built from pieces with +: ${short(textArg.getText(source))}` });
          else if (textArg) dynamic.push(`${at(node)}  ${name}(${short(textArg.getText(source))})`);
        }
        if (TRANSLATE.has(name)) flow(node);
      }
      if ((ts.isJsxSelfClosingElement(node) || ts.isJsxOpeningElement(node)) && node.tagName.getText(source) === 'T' && binding('T', node.tagName) !== 'other') {
        const attribute = name => node.attributes.properties.find(p => ts.isJsxAttribute(p) && p.name.getText(source) === name);
        const valueOf = attr => attr?.initializer && ts.isStringLiteral(attr.initializer) ? attr.initializer.text : literal(attr?.initializer?.expression);
        const value = valueOf(attribute('text'));
        const contextAttr = attribute('context');
        const context = contextAttr ? valueOf(contextAttr) : undefined;
        if (value === undefined) dynamic.push(`${at(node)}  <T text={…}>`);
        else if (!contextAttr) add(plainKey(value, node), 'rich', value, node);
        else if (contextOk(context, node)) add(`${context}|${value}`, 'rich', value, node);
      }
      ts.forEachChild(node, visit);
    };
    visit(source);
  }
  return { keys, problems, dynamic, areas, files: files.length };
}

const short = s => (s.length > 70 ? s.slice(0, 67) + '…' : s);
const placeholders = s => [...s.matchAll(/\{(\w+)\}/g)].map(x => x[1]).sort();
const tagNames = s => [...s.matchAll(/<(\w+)>/g)].map(x => x[1]).sort().join(',');

/** Compares the code with every language's area files. `lines` is the report. */
export async function checkI18n({ release = false, update = false, listDynamic = false, ts } = {}) {
  ts ??= (await import('typescript')).default;
  const { keys, problems, dynamic, areas, files } = await readSources(ts);
  const lines = [];
  let failing = 0, missingCount = 0;
  const plurals = [...keys.values()].filter(k => k.kind === 'plural').length, rich = [...keys.values()].filter(k => k.kind === 'rich').length;
  lines.push(`Source: ${keys.size} keys (${plurals} with a count, ${rich} with markup) in ${areas.size} areas, ${files} files read; ${dynamic.length} calls translate a value at run time.`);
  for (const p of problems) { failing++; lines.push(`  ${p.where}  ${p.text}`); }
  if (listDynamic) for (const line of dynamic) lines.push(`  dynamic: ${line}`);
  const languages = existsSync(localeRoot) ? readdirSync(localeRoot).filter(name => statSync(join(localeRoot, name)).isDirectory()).sort() : [];
  for (const language of languages) {
    const dir = join(localeRoot, language);
    const catalogs = new Map();
    for (const name of readdirSync(dir).filter(n => n.endsWith('.json')).sort()) {
      const relPath = relative(root, join(dir, name)).replaceAll('\\', '/');
      try { catalogs.set(name.replace(/\.json$/, ''), { path: join(dir, name), rel: relPath, entries: JSON.parse(readFileSync(join(dir, name), 'utf8')) }); }
      catch (error) { failing++; lines.push(`  ${relPath}  not valid JSON: ${error.message}`); }
    }
    let translated = 0, total = 0, languageMissing = 0;
    const out = [];
    // The same key translated two ways across area files.
    const seen = new Map();
    for (const [area, catalog] of catalogs) for (const [key, entry] of Object.entries(catalog.entries)) {
      if (entry === '' || (typeof entry === 'object' && !entry?.other)) continue;
      const value = JSON.stringify(entry);
      const before = seen.get(key);
      if (before && before.value !== value) { failing++; out.push(`  ${catalog.rel}  two ways: ${short(key)} (${before.area}.json has another translation)`); }
      else if (!before) seen.set(key, { area, value });
    }
    for (const [area, used] of [...areas].sort()) {
      const catalog = catalogs.get(area);
      const rel = catalog?.rel ?? `src/i18n/locales/${language}/${area}.json`;
      const missing = [];
      for (const key of used) {
        total++;
        const info = keys.get(key), entry = catalog?.entries[key];
        const empty = entry === undefined || entry === '' || (typeof entry === 'object' && (entry === null || !entry.other));
        if (empty) { missing.push(key); continue; }
        translated++;
        const where = `${rel}  ${short(key)}`;
        if ((info.kind === 'plural') !== (typeof entry === 'object')) { failing++; out.push(`  ${where}: ${info.kind === 'plural' ? 'needs plural forms (an object with "other")' : 'needs a plain string'}`); continue; }
        if (typeof entry === 'object') {
          const wanted = new Set(placeholders(info.english.other).filter(n => n !== 'count'));
          for (const [form, text] of Object.entries(entry)) {
            if (!PLURAL_FORMS.has(form) && !/^=\d+$/.test(form)) { failing++; out.push(`  ${where}: unknown plural form "${form}"`); }
            if (typeof text !== 'string') { failing++; out.push(`  ${where}: form "${form}" is not a string`); continue; }
            const got = placeholders(text).filter(n => n !== 'count');
            if (got.some(n => !wanted.has(n)) || [...wanted].some(n => !got.includes(n))) { failing++; out.push(`  ${where}: form "${form}" has {${got.join(',')}}, the English {${[...wanted].join(',')}}`); }
          }
        } else {
          const english = info.english, want = placeholders(english).join(','), got = placeholders(entry).join(',');
          if (want !== got) { failing++; out.push(`  ${where}: placeholders {${want}} but the translation has {${got}}`); }
          if (tagNames(english) !== tagNames(entry)) { failing++; out.push(`  ${where}: markup <${tagNames(english)}> but the translation has <${tagNames(entry)}>`); }
        }
      }
      missingCount += missing.length; languageMissing += missing.length;
      for (const key of missing) out.push(`  ${keys.get(key).firstIn[area]}  missing in ${rel}: ${short(key)}`);
      if (update && missing.length) {
        const next = { ...(catalog?.entries ?? {}) };
        for (const key of missing) next[key] = keys.get(key).kind === 'plural' ? { other: '' } : '';
        writeFileSync(catalog?.path ?? join(dir, `${area}.json`), JSON.stringify(next, null, 2) + '\n');
        out.push(`  added ${missing.length} empty entries to ${rel}`);
      }
    }
    for (const [area, catalog] of catalogs) {
      const used = areas.get(area) ?? new Set();
      for (const key of Object.keys(catalog.entries)) if (!used.has(key)) {
        failing++;
        const elsewhere = keys.get(key) ? ` (used in area ${[...keys.get(key).area].join(', ')})` : '';
        out.push(`  ${catalog.rel}  unused: ${short(key)}${elsewhere}`);
      }
    }
    lines.push(`${language}: ${translated} of ${total} translated in ${catalogs.size} area files; ${languageMissing} missing.`);
    lines.push(...out);
  }
  const failed = failing > 0 || (release && missingCount > 0);
  lines.push(failed ? `i18n check FAILED: ${failing} problem(s)${release ? `, ${missingCount} missing` : ''}.` : `i18n check passed${missingCount ? ` (${missingCount} missing entries; --release fails on them)` : ''}.`);
  return { failed, failing, missing: missingCount, lines, keys };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = new Set(process.argv.slice(2));
  if (args.has('--keys')) {
    const ts = (await import('typescript')).default;
    for (const key of (await readSources(ts)).keys.keys()) console.log(key);
  } else {
    const result = await checkI18n({ release: args.has('--release'), update: args.has('--update'), listDynamic: args.has('--dynamic') });
    for (const line of result.lines) console.log(line);
    if (result.failed) process.exit(1);
  }
}
