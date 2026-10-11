/// <reference types="vitest/config" />
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

const apiTarget = process.env.FD_API_URL ?? 'https://localhost:8443';

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: '../management/src/FileDock.Management.Api/wwwroot',
    emptyOutDir: true,
    sourcemap: true,
  },
  server: {
    proxy: {
      '/api': { target: apiTarget, changeOrigin: true, secure: false },
      '/hubs': { target: apiTarget, changeOrigin: true, secure: false, ws: true },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.{ts,tsx}', 'tests/**/*.test.{ts,tsx}'],
    setupFiles: ['./tests/setup.ts'],
  },
});
