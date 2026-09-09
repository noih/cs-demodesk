import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

const steamBase = '76561197960265728'; // Public SteamID encoding base, not a player.
function issues(text) {
  const found = [];
  if ([...text.matchAll(/(?<![\w.])7656\d{13}(?![\w.])/g)].some(m => m[0] !== steamBase)) found.push('SteamID64');
  if (/\bSTEAM_[0-5]:[01]:\d+\b|\[U:1:\d+\]/.test(text)) found.push('Steam account identifier');
  if (/steamcommunity\.com\/(?:profiles\/\d+|id\/[\w-]+)/i.test(text)) found.push('player profile URL');
  if ([...text.matchAll(/spec_lock_to_accountid\s+(\d+)/g)].some(m => m[1] !== '123')) found.push('hardcoded account ID');
  if (/[A-Z]:[\\/]+Users[\\/]+(?!Public\b|Default\b|<|\$|%)[\w.-]+/i.test(text)) found.push('personal Windows path');
  return found;
}
assert.deepEqual(issues(steamBase), []);
assert.ok(issues('7656' + '1'.repeat(13)).includes('SteamID64'));
assert.ok(issues('spec_lock_to_accountid ' + 456).includes('hardcoded account ID'));
assert.deepEqual(issues('spec_lock_to_accountid ' + 123), []);

const staged = process.argv.includes('--staged');
const git = (...args) => execFileSync('git', ['-c', `safe.directory=${process.cwd().replaceAll('\\','/')}`, ...args], { maxBuffer: 64 * 1024 * 1024 });
const files = [...new Set(git('ls-files', '-z', '--cached', ...(staged ? [] : ['--others', '--exclude-standard'])).toString('utf8').split('\0').filter(Boolean))];
const failures = [];
for (const file of files) {
  if (/^(?:out|target|research|demodesk-data)\//.test(file) || /\.dem(?:\.(?:gz|bz2))?$/.test(file)) {
    failures.push(`${file}: private capture/output path`); continue;
  }
  if (!staged && !existsSync(file)) continue;
  const bytes = staged ? git('show', `:${file}`) : readFileSync(file);
  if (bytes.includes(0)) continue;
  for (const issue of issues(bytes.toString('utf8'))) failures.push(`${file}: ${issue}`);
}
if (failures.length) {
  console.error(failures.join('\n')); // Report locations, never matching identities.
  process.exitCode = 1;
} else console.log(`Privacy patterns checked in ${files.length} files. Names and images still require review.`);
