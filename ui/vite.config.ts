import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const port = parseInt(process.env.VITE_PORT ?? '5173');

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    target: 'esnext',
    outDir: 'dist',
  },
});
