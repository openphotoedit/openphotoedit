import fs from 'node:fs';
const src = fs.readFileSync(process.argv[2], 'utf8');
const rows = []; let group = null;
for (const line of src.split('\n')) {
  const g = line.match(/^## ([A-S])\. (.+)$/); if (g) { group = { id: g[1], name: g[2] }; continue; }
  if (!group || !line.startsWith('| ') ) continue;
  const c = line.trim().slice(1, -1).split('|').map(s => s.trim());
  if (c.length !== 9 || !/^[A-S]\d+$/.test(c[0])) continue;
  const [id, feature, found, lite, pro, phase, ai, size, notes] = c;
  rows.push({ id, group: group.id, groupName: group.name, feature, found, lite, pro, phase, ai, size, notes });
}
const by = (k) => rows.reduce((m, r) => (m[k(r)] = (m[k(r)] || 0) + 1, m), {});
const groups = {};
for (const r of rows) {
  const g = groups[r.group] ??= { name: r.groupName, rows: 0, lite: 0, must: 0, model: 0 };
  g.rows++; if (r.lite.startsWith('●')) g.lite++; if (r.pro.startsWith('M')) g.must++; if (/T[123]/.test(r.ai)) g.model++;
}
const tot = Object.values(groups).reduce((a, g) => ({ rows: a.rows + g.rows, lite: a.lite + g.lite, must: a.must + g.must, model: a.model + g.model }), { rows: 0, lite: 0, must: 0, model: 0 });
const phaseKey = r => r.phase.includes('→') ? 'multi' : r.phase.replace(/ .*/, '');
console.log(JSON.stringify({ groups, tot, phases: by(phaseKey) }, null, 1));
if (process.argv[3]) fs.writeFileSync(process.argv[3], JSON.stringify(rows));
