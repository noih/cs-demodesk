// Copies the standalone Windows executable produced by `tauri build --no-bundle`
// into dist-portable/CS-DemoDesk-<version>.exe — that single .exe is the portable
// app (WebView2 comes with Windows 10/11).
import { copyFileSync, mkdirSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

const { version } = JSON.parse(readFileSync('package.json', 'utf8'));
const release = path.resolve('target/release');
const exe = readdirSync(release).find((f) => f.toLowerCase().endsWith('.exe') && !f.includes('build'));
if (!exe) {
  console.error('no .exe found in target/release — run `npm run app:build` on Windows');
  process.exit(1);
}
mkdirSync('dist-portable', { recursive: true });
const dest = path.join('dist-portable', `CS-DemoDesk-${version}.exe`);
copyFileSync(path.join(release, exe), dest);
console.log(`portable build → ${dest}`);
