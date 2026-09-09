// Usage: node scripts/export-recoil-reference.mjs <recording.dem> <reference.json>
// Uses the application's parser so exports and in-app references cannot drift.
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdtemp, readFile, writeFile, unlink, rmdir, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const [demoArg, outputArg] = process.argv.slice(2);
if (!demoArg || !outputArg || process.argv.length !== 4) {
  console.error('Usage: node scripts/export-recoil-reference.mjs <recording.dem> <reference.json>');
  process.exit(1);
}
const demo = resolve(demoArg), output = resolve(outputArg);
if (!output.toLowerCase().endsWith('.json') || demo.toLowerCase() === output.toLowerCase()) throw new Error('Output must be a separate .json file');
const before = await stat(demo);
const hash = createHash('sha256');
for await (const chunk of createReadStream(demo)) hash.update(chunk);
const scratch = await mkdtemp(join(tmpdir(), 'demodesk-recoil-'));
const parsedPath = join(scratch, 'parsed.json');
try {
  const run = spawnSync('cargo', ['run', '--quiet', '-p', 'demodesk-core', '--example', 'parse', '--', demo, parsedPath], {
    cwd: fileURLToPath(new URL('../', import.meta.url)), encoding: 'utf8', windowsHide: true,
  });
  if (run.error) throw run.error;
  if (run.status !== 0) throw new Error(run.stderr || run.stdout || `Parser exited ${run.status}`);
  const after = await stat(demo);
  if (after.size !== before.size || after.mtimeMs !== before.mtimeMs) throw new Error('Demo changed during export; retry with a completed recording');
  const parsed = JSON.parse(await readFile(parsedPath, 'utf8'));
  const weapons = parsed.recoilReference;
  if (!weapons || !Object.keys(weapons).length) throw new Error('No paired rifle bursts; no reference can be generated from this demo');
  const result = {
    schemaVersion: 1, method: 'same-tick-firing-minus-eye-angles-v1', estimated: true,
    generatedAt: new Date().toISOString(),
    source: { demo: basename(demo), sha256: hash.digest('hex'), map: parsed.info.mapName },
    units: 'degrees', axes: { x: 'right-positive', y: 'up-positive' },
    description: 'Compensation estimate pooled across fresh bursts in this demo. First shots align to zero. Subtick aim changes remain noise. Not an official ideal pattern; missing later shots are not extrapolated.',
    weapons,
  };
  await writeFile(output, JSON.stringify(result, null, 2) + '\n');
  console.log(`Wrote ${output}`);
  for (const gun of ['ak47', 'm4a1', 'm4a1_silencer']) {
    const points = weapons[gun];
    console.log(points?.length ? `${gun}: ${points.length} shots, ${points[0].samples} bursts, ${points.at(-1).samples} samples at last shot` : `${gun}: unavailable`);
  }
} finally {
  await unlink(parsedPath).catch(error => { if (error.code !== 'ENOENT') throw error; });
  await rmdir(scratch);
}
