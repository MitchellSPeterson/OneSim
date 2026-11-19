#!/usr/bin/env node
// Regenerates the SwiftUI area files in docs/compatibility/swiftui/ from Apple's SwiftUI + SwiftUICore
// .swiftinterface files in the installed iOS SDK (macOS + Xcode only; not run in CI).
//   node scripts/swiftui-inventory.mjs
// One row per public top-level type, View/Scene/Shape modifier, Text/Image method and EnvironmentValues
// property available on iOS. Status, Issue and Notes of rows already in the folder are kept; new rows get ✅ when
// OneSim's module (frameworks/swiftui/Sources/SwiftUI) declares the same name in the same scope, else ⬜.
// The SDK version used is written to docs/compatibility/versions.json (`ios.apiSdk`).
// Afterwards run `node scripts/api-coverage.mjs` to refresh the summary.
import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';

const root = new URL('..', import.meta.url).pathname;
const dir = root + 'docs/compatibility/swiftui/';
const sdk = execFileSync('xcrun', ['--sdk', 'iphoneos', '--show-sdk-path'], { encoding: 'utf8' }).trim();
const iface = (m) => readFileSync(`${sdk}/System/Library/Frameworks/${m}.framework/Modules/${m}.swiftmodule/arm64e-apple-ios.swiftinterface`, 'utf8');

const MEMBER_SCOPES = ['View', 'Scene', 'Shape', 'InsettableShape', 'Text', 'Image', 'EnvironmentValues'];
const strip = (s) => s.replace(/\b(?:SwiftUICore|SwiftUI|Swift|Foundation|CoreFoundation|CoreGraphics)(?:::|\.)/g, ''); // also `Module::Type` selectors

// Walks Swift source line by line, tracking `extension`/type scopes by brace depth.
// Yields { kind, name, scope, conforms, attrs } for top-level types and members of MEMBER_SCOPES.
function* decls(src) {
  const stack = []; // { name, depth, attrs }
  let depth = 0, attrs = [];
  for (const raw of src.split('\n')) {
    const line = strip(raw.trim());
    if (line.startsWith('@') && !/\b(func|var|struct|class|enum|protocol|actor|extension)\s+\w/.test(line.replace(/"(?:[^"\\]|\\.)*"/g, ''))) {
      attrs.push(line);
    } else if (!line.startsWith('#') && line) {
      const all = attrs.concat(line);
      const scope = stack.length ? stack[stack.length - 1] : null;
      let m;
      if ((m = line.match(/\bextension\s+([\w.]+)\s*(?::\s*([^{]+?))?\s*(?:where\b[^{]*)?\{/))) {
        const name = strip(m[1]);
        if (!scope && m[2]) yield { kind: 'conformance', name, conforms: strip(m[2]), attrs: all };
        stack.push({ name, depth, attrs: all, ext: true });
      } else if ((m = line.match(/\b(struct|class|enum|protocol|actor)\s+(\w+)(?:<[^{]*?>)?\s*(?::\s*([^{]+?))?\s*(?:where\b[^{]*)?\{/))) {
        if (!scope && /\b(public|open)\s/.test(line)) yield { kind: 'type', name: m[2], type: m[1], conforms: strip(m[3] || ''), attrs: all };
        stack.push({ name: scope ? scope.name + '.' + m[2] : m[2], depth, attrs: all });
      } else if (scope && MEMBER_SCOPES.includes(scope.name) && (m = line.match(/\b(func|var)\s+(`?\w+`?)/)) && !/\bstatic\b/.test(line) && /\b(public|open)\s/.test(line)) {
        yield { kind: m[1], name: m[2].replace(/`/g, ''), scope: scope.name, attrs: all.concat(scope.attrs) };
      }
      attrs = [];
    }
    for (const c of raw.replace(/"(?:[^"\\]|\\.)*"/g, '')) {
      if (c === '{') depth++;
      else if (c === '}') { depth--; while (stack.length && stack[stack.length - 1].depth >= depth) stack.pop(); }
    }
  }
}

const keyOf = (d) => (d.kind === 'type' ? d.name : d.scope === 'EnvironmentValues' ? `\\.${d.name}` : d.scope === 'View' ? `.${d.name}` : `${d.scope}.${d.name}`);

// OneSim's module: which keys it declares.
const ours = new Set();
const ourDir = root + 'frameworks/swiftui/Sources/SwiftUI/';
for (const f of readdirSync(ourDir).filter((f) => f.endsWith('.swift'))) {
  for (const d of decls(readFileSync(ourDir + f, 'utf8'))) if (d.kind !== 'conformance') ours.add(keyOf(d));
}

// Apple's API.
const avail = (attrs) => {
  const a = attrs.filter((s) => s.startsWith('@available'));
  if (a.some((s) => /@available\((iOS|\*)\s*,\s*unavailable/.test(s))) return null;
  if (attrs.some((s) => s.startsWith('@_spi'))) return null;
  const ios = a.find((s) => /@available\(iOS[ ,]/.test(s)) || '';
  const since = (ios.match(/iOS (\d+(?:\.\d+)?)/) || ios.match(/introduced: (\d+(?:\.\d+)?)/) || [])[1];
  const dep = (ios.match(/deprecated: (\d+(?:\.\d+)?)/) || [])[1];
  // Apple writes a soft deprecation ("will be deprecated") as `deprecated: 100000.0`.
  return { since: since ? parseFloat(since) : 13, deprecated: !dep ? null : parseFloat(dep) < 1000 ? `Deprecated in iOS ${dep.replace(/\.0$/, '')}` : 'Deprecated' };
};
const api = new Map(); // key -> { section, since, deprecated }
const conforms = new Map(); // type -> conformance text
const typeDecls = [];
for (const src of [iface('SwiftUICore'), iface('SwiftUI')]) {
  for (const d of decls(src)) {
    if (d.name.startsWith('_') || d.name === 'init' || d.name === 'body') continue;
    if (d.kind === 'conformance') { conforms.set(d.name, (conforms.get(d.name) || '') + ',' + d.conforms); continue; }
    const av = avail(d.attrs);
    if (!av) continue;
    if (d.kind === 'type') typeDecls.push({ ...d, av });
    else add(keyOf(d), d.scope === 'View' ? 'View modifiers' : d.scope === 'EnvironmentValues' ? 'Environment values' : d.scope === 'Scene' ? 'Scenes' : d.scope.endsWith('Shape') ? 'Shapes' : 'Text and Image methods', av);
  }
}
for (const d of typeDecls) {
  const c = d.conforms + (conforms.get(d.name) || '');
  const section = d.attrs.some((s) => s.includes('@propertyWrapper')) ? 'Property wrappers'
    : d.type === 'protocol' ? (/Style$/.test(d.name) ? 'Styles' : 'Protocols')
    : /Style$/.test(d.name) ? 'Styles'
    : /\b(Shape|InsettableShape)\b/.test(c) ? 'Shapes'
    : /\bGesture\b/.test(c) ? 'Gestures'
    : /\bView\b/.test(c) ? 'Views'
    : /\bScene\b/.test(c) ? 'Scenes'
    : 'Other types';
  add(d.name, section, d.av);
}
function add(key, section, av) {
  const cur = api.get(key);
  if (!cur) return api.set(key, { section, since: av.since, deprecated: av.deprecated });
  cur.since = Math.min(cur.since, av.since);
  if (!av.deprecated) cur.deprecated = null; // deprecated only when every overload is
}

// Existing rows (by API key) keep their status, issue and notes.
const kept = new Map();
for (const line of readdirSync(dir).filter((f) => f.endsWith('.md')).flatMap((f) => readFileSync(dir + f, 'utf8').split('\n'))) {
  const c = line.split('|').map((s) => s.trim());
  if (c.length >= 7 && c[1].startsWith('`')) kept.set(c[1].replace(/`/g, ''), { status: c[3], issue: c[4], notes: c[5] });
}

// Section -> area file (also listed in FRAMEWORKS in scripts/api-coverage.mjs).
const FILES = { Views: 'views', 'View modifiers': 'view-modifiers', 'Text and Image methods': 'text-and-image', Shapes: 'shapes', Styles: 'styles', Gestures: 'gestures', Scenes: 'scenes', 'Property wrappers': 'property-wrappers', 'Environment values': 'environment-values', Protocols: 'protocols', 'Other types': 'other-types' };
const SECTIONS = Object.keys(FILES);
const INTRO = 'Part of [SwiftUI API coverage](README.md): status legend, summary and how to add a row. Generated by `scripts/swiftui-inventory.mjs`; edit Status, Issue and Notes in place (they survive regeneration).';
const warnings = [];
for (const s of SECTIONS) {
  const rows = [...api].filter(([, v]) => v.section === s).sort(([a], [b]) => a.replace(/^\W+/, '').localeCompare(b.replace(/^\W+/, '')));
  writeFileSync(dir + FILES[s] + '.md', [`# ${s}`, '', INTRO, '', '| API | Since | iOS | Issue | Notes |', '|---|---|---|---|---|', ...rows.map(([k, v]) => {
    const has = ours.has(k);
    const row = kept.get(k) || { status: has ? '✅' : '⬜', issue: '', notes: '' };
    if (!row.notes && v.deprecated) row.notes = v.deprecated;
    if (row.status === '⬜' && has) warnings.push(`${k}: marked ⬜ but OneSim declares it`);
    return `| \`${k}\` | ${v.since.toFixed(1).replace(/\.0$/, '')} | ${row.status} | ${row.issue} | ${row.notes} |`;
  }), ''].join('\n'));
}
const matrixPath = root + 'docs/compatibility/versions.json';
const matrix = JSON.parse(readFileSync(matrixPath, 'utf8'));
matrix.ios.apiSdk = sdk.match(/iPhoneOS([\d.]+)\.sdk/)[1];
writeFileSync(matrixPath, JSON.stringify(matrix, null, 2) + '\n');
console.log(`${api.size} APIs (${[...api.keys()].filter((k) => ours.has(k)).length} declared by OneSim) from ${sdk}`);
for (const s of SECTIONS) console.log(`  ${s}: ${[...api.values()].filter((v) => v.section === s).length}`);
if (warnings.length) console.warn(warnings.join('\n'));
