import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
	plugins: [react(), tailwindcss()],
	clearScreen: false,
	server: {
		port: 1420,
		strictPort: true,
		watch: { ignored: ['**/src-tauri/**'] }
	},
	build: {
		target: 'es2021',
		outDir: 'dist'
	}
});
