/// <reference types="vitest/config" />
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

const apiTarget = process.env.FD_API_URL ?? 'http://localhost:8021';

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: '../management/src/FileDock.Management.Api/wwwroot',
    emptyOutDir: true,
    sourcemap: true,
  },
  server: {
    proxy: {
      '/api': { target: apiTarget, changeOrigin: true },
      '/hubs': { target: apiTarget, changeOrigin: true, ws: true },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.{ts,tsx}', 'tests/**/*.test.{ts,tsx}'],
    setupFiles: ['./tests/setup.ts'],
  },
});
