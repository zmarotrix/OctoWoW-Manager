import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@fontsource-variable/inter';
import { App } from './App';
import { SettingsProvider } from './lib/settings';
import './index.css';

const queryClient = new QueryClient({
	defaultOptions: {
		// The core pushes `state-changed` events, so never refetch on a timer or on focus.
		queries: { staleTime: Infinity, refetchOnWindowFocus: false, retry: false }
	}
});

const start = async () => {
	// Plain-browser preview: stand in for the Rust backend.
	if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) await import('./lib/devMock');

	createRoot(document.getElementById('root')!).render(
		<StrictMode>
			<QueryClientProvider client={queryClient}>
				<SettingsProvider>
					<App />
				</SettingsProvider>
			</QueryClientProvider>
		</StrictMode>
	);
};

start();
