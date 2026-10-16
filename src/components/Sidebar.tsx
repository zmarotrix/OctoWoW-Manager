import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { ArrowUpCircle, CloudDownload, Cpu, Layers, type LucideIcon, Package, Play, Save, SlidersHorizontal } from 'lucide-react';
import { useJobs } from '../lib/jobs';
import logo from '../assets/logo.ico';
import { api, errorText } from '../lib/ipc';
import { useSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import { cn, Dot } from './ui';

export type TabId = 'settings' | 'tweaks' | 'gameMods' | 'addons' | 'updater';

export const TABS: { id: TabId; icon: LucideIcon; label: string }[] = [
	{ id: 'settings', icon: SlidersHorizontal, label: 'Game Settings' },
	{ id: 'tweaks', icon: Cpu, label: 'Client Tweaks' },
	{ id: 'gameMods', icon: Layers, label: 'Game Mods' },
	{ id: 'addons', icon: Package, label: 'Addons' },
	{ id: 'updater', icon: CloudDownload, label: 'Game Updates' }
];

const useCounts = (wowDir: string): Partial<Record<TabId, number>> => {
	const mods = useQuery({ queryKey: ['gameMods', wowDir], queryFn: api.scanGameMods, enabled: !!wowDir });
	const addons = useQuery({ queryKey: ['addons', wowDir], queryFn: api.scanAddons, enabled: !!wowDir });
	const client = useQuery({ queryKey: ['clientUpdate'], queryFn: api.clientStatus });
	return { gameMods: mods.data?.length, addons: addons.data?.length, updater: client.data?.downloadFiles || undefined };
};

const ClientStatus = () => {
	const { settings } = useSettings();
	const info = useQuery({
		queryKey: ['client', settings.wowDir],
		queryFn: () => api.validateGameDir(settings.wowDir),
		enabled: !!settings.wowDir
	});
	const folder = settings.wowDir.split(/[\\/]/).filter(Boolean).pop();
	const tone = !settings.wowDir ? 'muted' : info.data?.valid ? 'success' : 'warning';
	const status = !settings.wowDir ? 'No game folder set' : info.data?.valid ? 'Client ready' : (info.data?.message ?? 'Checking…');

	return (
		<div className="rounded-lg border border-line bg-panel px-3 py-2.5">
			<div className="flex items-center gap-2 text-xs font-medium">
				<Dot tone={tone} />
				{status}
			</div>
			{folder && <div className="mt-0.5 truncate pl-3.5 font-mono text-[11px] text-subtle" title={settings.wowDir}>{folder}</div>}
		</div>
	);
};

export const Sidebar = ({ tab, onTab }: { tab: TabId; onTab: (t: TabId) => void }) => {
	const { settings, update } = useSettings();
	const counts = useCounts(settings.wowDir);
	const jobs = useJobs();
	const [applyJob, setApplyJob] = useState<number | null>(null);
	const appUpdate = useQuery({ queryKey: ['appUpdate'], queryFn: api.checkAppUpdate, retry: false });

	// Clear the "unapplied changes" dot once an Apply finishes successfully.
	useEffect(() => {
		const j = jobs.find(x => x.id === applyJob);
		if (!j || j.status === 'running') return;
		if (j.status === 'finished') {
			update(s => ({ ...s, pendingApply: false }));
			toast.success(j.message);
		} else toast.error(j.message);
		setApplyJob(null);
	}, [jobs, applyJob, update]);

	// Flush the latest settings first: saves are debounced.
	const apply = () =>
		api
			.saveSettings(settings)
			.then(api.applyChanges)
			.then(setApplyJob)
			.catch(e => toast.error(errorText(e)));
	const play = () => api.launchGame().catch(e => toast.error(errorText(e)));

	return (
		<aside className="flex w-[232px] shrink-0 flex-col border-r border-line bg-sidebar">
			<div data-tauri-drag-region className="flex h-16 items-center gap-2.5 px-4 pt-2">
				<img src={logo} alt="" className="pointer-events-none h-8 w-8 rounded-lg" />
				<div className="pointer-events-none leading-tight">
					<div className="text-sm font-semibold tracking-tight">
						Octo<span className="bg-gradient-to-r from-accent to-accent-2 bg-clip-text text-transparent">WoW</span>
					</div>
					<div className="text-[11px] text-subtle">Manager</div>
				</div>
			</div>

			<nav className="mt-3 flex flex-col gap-0.5 px-2.5">
				<div className="px-2.5 pb-1.5 text-[11px] font-medium tracking-wide text-subtle uppercase">Manage</div>
				{TABS.map(t => {
					const active = tab === t.id;
					const count = counts[t.id];
					return (
						<button
							key={t.id}
							onClick={() => onTab(t.id)}
							className={cn(
								'group relative flex h-9 items-center gap-2.5 rounded-lg px-2.5 text-[13px] font-medium transition-colors duration-75',
								active ? 'bg-white/[0.07] text-fg' : 'text-muted hover:bg-white/[0.035] hover:text-fg'
							)}
						>
							{active && <span className="brand-gradient absolute top-2 bottom-2 -left-2.5 w-[3px] rounded-r-full" />}
							<t.icon size={16} strokeWidth={1.75} className={active ? 'text-accent' : 'text-subtle group-hover:text-muted'} />
							{t.label}
							{count !== undefined &&
								(t.id === 'updater' ? (
									<span className="ml-auto rounded-full bg-warning/15 px-1.5 font-mono text-[11px] text-amber-400 tabular-nums" title="Files to update">{count}</span>
								) : (
									<span className="ml-auto font-mono text-[11px] text-subtle tabular-nums">{count}</span>
								))}
						</button>
					);
				})}
			</nav>

			<div className="mt-auto flex flex-col gap-2 p-3">
				{appUpdate.data?.newer && (
					<button
						onClick={() => openUrl(appUpdate.data!.url)}
						className="flex items-center gap-2 rounded-lg border border-warning/30 bg-warning/10 px-3 py-2 text-left text-xs font-medium text-amber-300 hover:bg-warning/15"
					>
						<ArrowUpCircle size={14} />
						Manager {appUpdate.data.latest} is available
					</button>
				)}
				<ClientStatus />
				<button
					onClick={apply}
					disabled={applyJob !== null}
					className="flex h-9 disabled:opacity-60 items-center justify-center gap-2 rounded-lg border border-line-strong bg-white/[0.04] text-[13px] font-medium transition-colors hover:bg-white/[0.08] active:scale-[0.98]"
				>
					<Save size={15} strokeWidth={1.75} className="text-muted" />
					Apply Changes
					{settings.pendingApply && <span title="You have unapplied changes" className="h-1.5 w-1.5 rounded-full bg-warning shadow-[0_0_8px] shadow-warning" />}
				</button>
				<button
					onClick={play}
					className="brand-gradient group relative flex h-12 items-center justify-center gap-2 overflow-hidden rounded-xl text-sm font-semibold tracking-wide text-white shadow-[inset_0_1px_0_rgb(255_255_255/0.25),0_8px_24px_-6px_rgb(139_92_246/0.7)] transition-[filter,transform] hover:brightness-110 active:scale-[0.98]"
				>
					<span className="absolute inset-0 -translate-x-full bg-gradient-to-r from-transparent via-white/20 to-transparent transition-transform duration-700 group-hover:translate-x-full" />
					<Play size={16} fill="currentColor" />
					PLAY
				</button>
			</div>
		</aside>
	);
};
