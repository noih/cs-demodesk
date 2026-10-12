import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  base: './',
  clearScreen: false,
  optimizeDeps: { entries: ['index.html'] },
  server: {
    port: 4601,
    strictPort: true,
    // Native builds and large diagnostic captures are not frontend source files.
    watch: { ignored: ['**/target/**', '**/src-tauri/**', '**/vendor/**', '**/demodesk-data/**', '**/out/**', '**/research/**'] },
  },
  build: { target: 'es2022', outDir: 'dist', emptyOutDir: true },
});
