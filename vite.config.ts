import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: '127.0.0.1' },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: { target: 'es2022' },
  // Full-app UI tests take several seconds on 2-core CI runners; the 5 s
  // default made them fail by chance there. Only src/ holds tests: copies of
  // the project inside it (agent worktrees under .claude/) are not collected.
  test: { environment: 'jsdom', setupFiles: './src/test-setup.ts', restoreMocks: true, testTimeout: 30000, include: ['src/**/*.test.{ts,tsx}'] },
});
