// Copies the standalone Windows executable produced by `tauri build --no-bundle`
// into dist-portable/CS-DemoDesk-<version>.exe — that single .exe is the portable
// app (WebView2 comes with Windows 10/11).
import { copyFileSync, mkdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

const { version } = JSON.parse(readFileSync('package.json', 'utf8'));
const exe = path.resolve(process.argv[2] ?? 'target/release/demodesk.exe');
mkdirSync('dist-portable', { recursive: true });
const dest = path.join('dist-portable', `CS-DemoDesk-${version}.exe`);
copyFileSync(exe, dest);
console.log(`portable build → ${dest}`);
