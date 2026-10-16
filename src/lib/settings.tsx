import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { api, errorText, type Settings } from './ipc';
import { toast } from './toast';

const SAVE_DELAY_MS = 300;

type Ctx = {
	settings: Settings;
	/** Change settings that take effect immediately (e.g. game folder). */
	update: (fn: (s: Settings) => Settings) => void;
	/** Change settings that only take effect after Apply Changes. */
	stage: (fn: (s: Settings) => Settings) => void;
};

const SettingsContext = createContext<Ctx | null>(null);

export const SettingsProvider = ({ children }: { children: ReactNode }) => {
	const [settings, setSettings] = useState<Settings | null>(null);
	const timer = useRef<number>();

	useEffect(() => {
		api.getSettings().then(setSettings, e => toast.error(`Couldn't load settings: ${errorText(e)}`));
	}, []);

	// UI state updates instantly; disk writes are debounced.
	const update = useCallback((fn: (s: Settings) => Settings) => {
		setSettings(prev => {
			if (!prev) return prev;
			const next = fn(prev);
			window.clearTimeout(timer.current);
			timer.current = window.setTimeout(
				() => api.saveSettings(next).catch(e => toast.error(`Couldn't save settings: ${errorText(e)}`)),
				SAVE_DELAY_MS
			);
			return next;
		});
	}, []);

	const stage = useCallback(
		(fn: (s: Settings) => Settings) => update(s => ({ ...fn(s), pendingApply: true })),
		[update]
	);

	if (!settings) return null;
	return <SettingsContext.Provider value={{ settings, update, stage }}>{children}</SettingsContext.Provider>;
};

export const useSettings = () => {
	const ctx = useContext(SettingsContext);
	if (!ctx) throw new Error('useSettings outside SettingsProvider');
	return ctx;
};
