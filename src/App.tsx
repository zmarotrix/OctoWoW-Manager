import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { Upload } from 'lucide-react';
import { Sidebar, TABS, type TabId } from './components/Sidebar';
import { TitleBar } from './components/TitleBar';
import { JobBar } from './components/JobBar';
import { useCoreEvents } from './lib/jobs';
import { toast, Toasts } from './lib/toast';
import { ConfirmHost } from './lib/dialog';
import { useSettings } from './lib/settings';
import { GameSettingsPage } from './pages/GameSettings';
import { ClientTweaksPage } from './pages/ClientTweaks';
import { GameModsPage, queueMpqs } from './pages/GameMods';
import { AddonsPage, installZips } from './pages/Addons';
import { addCustomDll } from './pages/ClientTweaks';
import { GameUpdatesPage } from './pages/GameUpdates';

const PAGES: Record<TabId, () => JSX.Element> = {
	settings: GameSettingsPage,
	tweaks: ClientTweaksPage,
	gameMods: GameModsPage,
	addons: AddonsPage,
	updater: GameUpdatesPage
};

export const App = () => {
	const [tab, setTab] = useState<TabId>('settings');
	const [dragging, setDragging] = useState(false);
	const { settings, stage } = useSettings();
	useCoreEvents(useQueryClient());

	// Route dropped files like v2.6: .mpq → Game Mods, .dll → Client Tweaks, .zip → Addons.
	const handleDrop = useRef<(paths: string[]) => void>(() => undefined);
	handleDrop.current = (paths: string[]) => {
		const by = (ext: string) => paths.filter(p => p.toLowerCase().endsWith(ext));
		const [mpqs, dlls, zips] = [by('.mpq'), by('.dll'), by('.zip')];
		if (!mpqs.length && !dlls.length && !zips.length) return toast.error('Only .mpq, .dll and .zip files can be dropped.');
		if (!settings.wowDir) return toast.error('Set your WoW folder in Game Settings first.');
		if (zips.length) {
			setTab('addons');
			installZips(zips);
		}
		if (dlls.length) {
			setTab('tweaks');
			dlls.reduce((p, d) => p.then(() => addCustomDll(d, stage)), Promise.resolve());
		}
		if (mpqs.length) {
			setTab('gameMods');
			queueMpqs(mpqs);
		}
	};

	useEffect(() => {
		const unlisten = getCurrentWebview().onDragDropEvent(e => {
			const p = e.payload;
			if (p.type === 'enter' || p.type === 'over') return setDragging(true);
			setDragging(false);
			if (p.type === 'drop') handleDrop.current(p.paths);
		});
		return () => {
			unlisten.then(f => f());
		};
	}, []);

	return (
		<div className="flex h-full">
			<Sidebar tab={tab} onTab={setTab} />
			<main className="glow relative flex min-w-0 flex-1 flex-col">
				<TitleBar title={TABS.find(t => t.id === tab)?.label ?? ''} />
				{/* All pages stay mounted so switching tabs is instant and keeps scroll position. */}
				{TABS.map(({ id }) => {
					const Page = PAGES[id];
					return (
						<div key={id} className={tab === id ? 'animate-page flex-1 overflow-y-auto' : 'hidden'}>
							<div className="mx-auto max-w-[880px] px-8 pt-4 pb-32">
								<Page />
							</div>
						</div>
					);
				})}
				<JobBar />
			</main>
			{dragging && (
				<div className="pointer-events-none fixed inset-0 z-40 flex items-center justify-center bg-bg/70 p-4 backdrop-blur-sm">
					<div className="animate-enter flex h-full w-full flex-col items-center justify-center gap-3 rounded-2xl border-2 border-dashed border-accent/60 bg-accent/[0.04]">
						<div className="brand-gradient flex h-14 w-14 items-center justify-center rounded-2xl text-white shadow-[0_8px_30px_-6px_rgb(139_92_246/0.8)]">
							<Upload size={24} />
						</div>
						<div className="text-base font-semibold">Drop to install</div>
						<div className="text-xs text-muted">.mpq patches, .dll plugins, or .zip addons</div>
					</div>
				</div>
			)}
			<Toasts />
			<ConfirmHost />
		</div>
	);
};
