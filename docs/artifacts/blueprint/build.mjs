// Builds index.html: parses docs/feature-matrix.md into rows.json, then inlines rows.json and rows.zh.json into template.html.
// Usage: node docs/artifacts/blueprint/build.mjs
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const dir = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(dir, '../../..');
execFileSync('node', [path.join(repo, 'scripts/matrix-counts.mjs'), path.join(repo, 'docs/feature-matrix.md'), path.join(dir, 'rows.json')], { stdio: 'ignore' });

const rows = JSON.parse(fs.readFileSync(path.join(dir, 'rows.json'), 'utf8'));
const zh = JSON.parse(fs.readFileSync(path.join(dir, 'rows.zh.json'), 'utf8'));
const missing = rows.filter(r => !zh.rows[r.id]).map(r => r.id);
if (missing.length) { console.error('Missing Chinese for', missing.join(', ')); process.exit(1); }

const inline = o => JSON.stringify(o).replace(/</g, '\\u003c');
const html = fs.readFileSync(path.join(dir, 'template.html'), 'utf8')
  .replace('__ROWS__', () => inline(rows))
  .replace('__ZH__', () => inline(zh));
fs.writeFileSync(path.join(dir, 'index.html'), html);
console.log(`index.html: ${rows.length} rows, ${(html.length / 1024).toFixed(0)} KB`);
