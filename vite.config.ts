import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  base: './',
  clearScreen: false,
  server: {
    port: 4601,
    strictPort: true,
    // Cargo writes into target/ while the dev server runs; watching it hits a locked .dll (EBUSY).
    watch: { ignored: ['**/target/**', '**/src-tauri/**', '**/vendor/**', '**/demodesk-data/**'] },
  },
  build: { target: 'es2022', outDir: 'dist', emptyOutDir: true },
});
