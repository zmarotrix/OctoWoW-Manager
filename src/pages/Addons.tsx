import { useDeferredValue, useEffect, useMemo, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { ArrowDownToLine, ArrowUpRight, FileArchive, FolderX, GitBranch, Loader2, Package, Plus, RefreshCw, Search, Star, Trash2 } from 'lucide-react';
import { api, errorText, repoKey, type AddonEntry, type CatalogEntry } from '../lib/ipc';
import { useSettings } from '../lib/settings';
import { useJobs } from '../lib/jobs';
import { toast } from '../lib/toast';
import { confirm } from '../lib/dialog';
import { Badge, Button, cn, Empty, Input, PageHeader, Segmented } from '../components/ui';

// Stable per-addon hue for the avatar tile.
const hue = (s: string) => [...s].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);

const Avatar = ({ name, dim }: { name: string; dim?: boolean }) => {
	const h = hue(name);
	return (
		<div
			className={cn('flex h-10 w-10 shrink-0 items-center justify-center rounded-xl text-[13px] font-semibold text-white/90', dim && 'opacity-40 grayscale')}
			style={{ background: `linear-gradient(135deg, hsl(${h} 55% 42%), hsl(${(h + 40) % 360} 60% 30%))` }}
		>
			{name.replace(/[^A-Za-z0-9]/g, '').slice(0, 2) || '?'}
		</div>
	);
};

const ago = (iso: string | null) => {
	if (!iso) return '';
	const days = (Date.now() - new Date(iso).getTime()) / 86_400_000;
	if (days < 1) return 'today';
	if (days < 30) return `${Math.round(days)}d ago`;
	if (days < 365) return `${Math.round(days / 30)}mo ago`;
	return `${Math.round(days / 365)}y ago`;
};

/** Install / update / remove, shared by both views, with per-item busy state. */
const useAddonActions = () => {
	const { settings, update } = useSettings();
	const [busy, setBusy] = useState<Set<string>>(new Set());
	const mark = (key: string, on: boolean) =>
		setBusy(b => {
			const n = new Set(b);
			if (on) n.add(key);
			else n.delete(key);
			return n;
		});

	const track = (url: string) =>
		update(s => (s.trackedAddons.some(u => repoKey(u) === repoKey(url)) ? s : { ...s, trackedAddons: [...s.trackedAddons, url] }));
	const untrack = (url: string) => update(s => ({ ...s, trackedAddons: s.trackedAddons.filter(u => repoKey(u) !== repoKey(url)) }));

	const install = async (url: string, verb = 'Installed') => {
		const key = repoKey(url);
		mark(key, true);
		try {
			const r = await api.addonInstall(url);
			track(r.url);
			toast.success(`${verb} ${r.folders.join(', ')}`);
			return true;
		} catch (e) {
			toast.error(errorText(e));
			return false;
		} finally {
			mark(key, false);
		}
	};

	const remove = async (a: AddonEntry, all: AddonEntry[]) => {
		const ok = await confirm({ title: `Uninstall ${a.title}?`, message: `The ${a.folder} folder will be deleted from Interface/AddOns.`, confirmLabel: 'Uninstall', danger: true });
		if (!ok) return;
		try {
			if (!a.missing) await api.addonRemove(a.folder);
			// Stop tracking the repo once none of its folders remain.
			if (a.url && !all.some(o => o !== a && !o.missing && o.url && repoKey(o.url) === repoKey(a.url!))) untrack(a.url);
		} catch (e) {
			toast.error(errorText(e));
		}
	};

	return { busy, install, remove, settings };
};

const AddonCard = ({ a, busy, onUpdate, onRemove }: { a: AddonEntry; busy: boolean; onUpdate: () => void; onRemove: () => void }) => (
	<div className={cn('lazy-row group flex items-center gap-3.5 rounded-xl border border-line bg-panel px-4 py-3 transition-colors hover:bg-panel-hover', a.missing && 'border-danger/30')}>
		<Avatar name={a.title} dim={a.missing} />
		<div className="min-w-0 flex-1">
			<div className="flex items-center gap-2">
				<span className="truncate text-[13px] font-semibold">{a.title}</span>
				{a.missing && <Badge tone="danger">Missing files</Badge>}
				{a.isGit && (
					<Badge tone="info">
						<GitBranch size={11} className="mr-1" />
						git
					</Badge>
				)}
				{!a.managed && <Badge>Local</Badge>}
			</div>
			<div className="truncate font-mono text-[11px] text-subtle">
				{[a.version && `v${a.version}`, a.author, a.folder].filter(Boolean).join(' · ')}
			</div>
			{a.notes && <div className="mt-0.5 truncate text-xs text-muted">{a.notes}</div>}
		</div>
		{(a.updateAvailable || a.missing) && a.url && (
			<Button variant="primary" size="sm" icon={busy ? Loader2 : ArrowDownToLine} onClick={onUpdate} disabled={busy} className={busy ? '[&_svg]:animate-spin' : ''}>
				{a.missing ? 'Repair' : 'Update'}
			</Button>
		)}
		{a.url && (
			<Button variant="ghost" size="sm" icon={ArrowUpRight} title="Open repository" className="opacity-0 group-hover:opacity-100" onClick={() => openUrl(a.url!.replace(/\.git$/, ''))} />
		)}
		<Button variant="danger" size="sm" icon={Trash2} title="Uninstall" className="opacity-0 group-hover:opacity-100" onClick={onRemove} />
	</div>
);

const SectionTitle = ({ title, count, tone, action }: { title: string; count: number; tone?: string; action?: React.ReactNode }) => (
	<div className="mt-6 mb-2.5 flex items-center justify-between first:mt-0">
		<h2 className={cn('text-[13px] font-semibold', tone)}>
			{title} <span className="ml-1 font-mono text-[11px] text-subtle">{count}</span>
		</h2>
		{action}
	</div>
);

const InstalledView = ({ query, actions }: { query: string; actions: ReturnType<typeof useAddonActions> }) => {
	const { settings } = actions;
	const addons = useQuery({ queryKey: ['addons', settings.wowDir], queryFn: api.scanAddons, enabled: !!settings.wowDir });
	const checking = useJobs().some(j => j.label === 'Checking addons for updates' && j.status === 'running');
	const checked = useRef<string | null>(null);
	const all = addons.data ?? [];

	// Check for updates once per session (and per game folder), like v2.6 did on load.
	useEffect(() => {
		if (addons.data && checked.current !== settings.wowDir && all.some(a => a.url)) {
			checked.current = settings.wowDir;
			api.checkAddonUpdates().catch(() => undefined);
		}
	}, [addons.data, settings.wowDir]); // eslint-disable-line react-hooks/exhaustive-deps

	const match = (a: AddonEntry) => !query || a.title.toLowerCase().includes(query) || a.folder.toLowerCase().includes(query);
	const updates = all.filter(a => (a.updateAvailable || a.missing) && a.url && match(a));
	const installed = all.filter(a => !(a.updateAvailable || a.missing) || !a.url).filter(match);

	const updateAll = async () => {
		const urls = [...new Map(updates.map(a => [repoKey(a.url!), a.url!])).values()];
		for (const u of urls) await actions.install(u, 'Updated');
	};

	if (!settings.wowDir)
		return (
			<Empty icon={FolderX} title="No game folder">
				Please set a valid WoW directory in Game Settings to manage addons.
			</Empty>
		);
	if (addons.error)
		return (
			<Empty icon={FolderX} title="Couldn't read AddOns folder">
				{errorText(addons.error)}
			</Empty>
		);
	if (addons.isLoading)
		return (
			<div className="flex flex-col gap-2">
				{Array.from({ length: 5 }, (_, i) => (
					<div key={i} className="flex items-center gap-3.5 rounded-xl border border-line bg-panel px-4 py-3">
						<div className="h-10 w-10 animate-pulse rounded-xl bg-white/[0.05]" />
						<div className="flex-1 space-y-1.5">
							<div className="h-3 w-40 animate-pulse rounded bg-white/[0.06]" />
							<div className="h-2.5 w-64 animate-pulse rounded bg-white/[0.04]" />
						</div>
					</div>
				))}
			</div>
		);
	if (all.length === 0)
		return (
			<Empty icon={Package} title="No addons found">
				Install one from a Git URL above, browse the catalog, or drop a .zip file onto the window.
			</Empty>
		);
	if (updates.length + installed.length === 0)
		return <Empty icon={Search} title="No addons match your search." />;

	return (
		<>
			{updates.length > 0 && (
				<>
					<SectionTitle
						title="Updates Available"
						count={updates.length}
						tone="text-amber-400"
						action={
							<Button size="sm" icon={ArrowDownToLine} onClick={updateAll} disabled={actions.busy.size > 0}>
								Update All
							</Button>
						}
					/>
					<div className="flex flex-col gap-2">
						{updates.map(a => (
							<AddonCard key={a.folder + a.missing} a={a} busy={actions.busy.has(repoKey(a.url!))} onUpdate={() => actions.install(a.url!, a.missing ? 'Repaired' : 'Updated')} onRemove={() => actions.remove(a, all)} />
						))}
					</div>
				</>
			)}
			<SectionTitle
				title="Installed Addons"
				count={installed.length}
				action={checking && <span className="flex items-center gap-1.5 text-xs text-subtle"><Loader2 size={12} className="animate-spin" />Checking for updates…</span>}
			/>
			<div className="flex flex-col gap-2">
				{installed.map(a => (
					<AddonCard key={a.folder} a={a} busy={false} onUpdate={() => undefined} onRemove={() => actions.remove(a, all)} />
				))}
			</div>
		</>
	);
};

const BrowseView = ({ query, actions }: { query: string; actions: ReturnType<typeof useAddonActions> }) => {
	const { settings } = actions;
	const catalog = useQuery({ queryKey: ['addonCatalog'], queryFn: () => api.addonCatalog() });
	const addons = useQuery({ queryKey: ['addons', settings.wowDir], queryFn: api.scanAddons, enabled: !!settings.wowDir });

	const installed = useMemo(() => {
		const keys = new Set<string>();
		for (const a of addons.data ?? []) {
			if (a.missing) continue;
			if (a.url) keys.add(repoKey(a.url));
			keys.add('folder:' + a.folder.toLowerCase());
		}
		return keys;
	}, [addons.data]);
	const isInstalled = (c: CatalogEntry) => installed.has(repoKey(c.git)) || installed.has('folder:' + c.name.toLowerCase());

	const list = (catalog.data ?? []).filter(
		c => !query || c.title.toLowerCase().includes(query) || c.name.toLowerCase().includes(query) || c.description.toLowerCase().includes(query) || c.author.toLowerCase().includes(query)
	);

	if (catalog.error)
		return (
			<Empty icon={FolderX} title="Couldn't load the addon catalog">
				{errorText(catalog.error)}
			</Empty>
		);
	if (catalog.isLoading) return <Empty icon={Loader2} title="Loading the OctoWoW addon catalog…" />;
	if (list.length === 0) return <Empty icon={Search} title="No catalog addons match your search." />;

	return (
		<>
			<p className="mb-3 text-xs text-subtle">
				{catalog.data!.length} curated addons from the OctoWoW catalog. Installs track their repository, so they update like any other managed addon.
			</p>
			<div className="grid grid-cols-1 gap-2 xl:grid-cols-2">
				{list.map(c => {
					const done = isInstalled(c);
					const busy = actions.busy.has(repoKey(c.git));
					return (
						<div key={c.git} className="lazy-row group flex items-center gap-3.5 rounded-xl border border-line bg-panel px-4 py-3 transition-colors hover:bg-panel-hover">
							<Avatar name={c.title} />
							<div className="min-w-0 flex-1">
								<div className="flex items-center gap-2">
									<button onClick={() => openUrl(c.git.replace(/\.git$/, ''))} className="truncate text-left text-[13px] font-semibold hover:underline">
										{c.title}
									</button>
									{c.stars ? (
										<span className="flex items-center gap-0.5 text-[11px] text-subtle">
											<Star size={10} />
											{c.stars}
										</span>
									) : null}
								</div>
								<div className="truncate font-mono text-[11px] text-subtle">{[c.author || c.owner, c.version && `v${c.version}`, c.lastUpdated && `updated ${ago(c.lastUpdated)}`].filter(Boolean).join(' · ')}</div>
								<div className="mt-0.5 line-clamp-2 text-xs text-muted">{c.description}</div>
							</div>
							{done ? (
								<Badge tone="success">Installed</Badge>
							) : (
								<Button size="sm" icon={busy ? Loader2 : Plus} disabled={busy || !settings.wowDir} onClick={() => actions.install(c.git)} className={busy ? '[&_svg]:animate-spin' : ''}>
									Install
								</Button>
							)}
						</div>
					);
				})}
			</div>
		</>
	);
};

/** Installs dropped / picked .zip addons (used by drag & drop too). */
export const installZips = async (paths: string[]) => {
	for (const p of paths) {
		try {
			const folders = await api.addonInstallZip(p);
			toast.success(`Installed ${folders.join(', ')}`);
		} catch (e) {
			toast.error(`${p.split(/[\\/]/).pop()}: ${errorText(e)}`);
		}
	}
};

export const AddonsPage = () => {
	const actions = useAddonActions();
	const { settings } = actions;
	const [view, setView] = useState<'installed' | 'browse'>('installed');
	const [url, setUrl] = useState('');
	const [search, setSearch] = useState('');
	const query = useDeferredValue(search.trim().toLowerCase());
	const installingUrl = url.trim() && actions.busy.has(repoKey(url));

	const installUrl = async () => {
		if (!url.trim()) return;
		if (await actions.install(url.trim())) setUrl('');
	};
	const pickZip = async () => {
		const picked = await open({ multiple: true, title: 'Install addon from .zip', filters: [{ name: 'Zip archives', extensions: ['zip'] }] });
		if (picked) await installZips(Array.isArray(picked) ? picked : [picked]);
	};

	return (
		<>
			<PageHeader
				title="Addon Manager"
				description="Install directly from Git repositories, or drop a .zip file. Updates are fully automatic."
				actions={
					<>
						<Segmented
							value={view}
							options={[
								{ value: 'installed', label: 'Installed' },
								{ value: 'browse', label: 'Browse' }
							]}
							onChange={setView}
						/>
						<Button icon={RefreshCw} onClick={() => api.checkAddonUpdates().catch(e => toast.error(errorText(e)))} disabled={!settings.wowDir} title="Refresh list and check for updates" />
					</>
				}
			/>

			<div className="mb-5 flex gap-3">
				<div className="flex flex-1 gap-2 rounded-xl border border-line bg-panel p-2">
					<Input
						className="flex-1"
						icon={GitBranch}
						value={url}
						onChange={e => setUrl(e.target.value)}
						onKeyDown={e => e.key === 'Enter' && installUrl()}
						placeholder="Paste URL (e.g., https://github.com/user/addon)"
					/>
					<Button variant="primary" icon={installingUrl ? Loader2 : Plus} onClick={installUrl} disabled={!url.trim() || !settings.wowDir || !!installingUrl} className={installingUrl ? '[&_svg]:animate-spin' : ''}>
						{installingUrl ? 'Installing…' : 'Install Addon'}
					</Button>
					<Button icon={FileArchive} onClick={pickZip} disabled={!settings.wowDir} title="Install from a .zip file" />
				</div>
				<div className="rounded-xl border border-line bg-panel p-2">
					<Input icon={Search} className="w-52" value={search} onChange={e => setSearch(e.target.value)} placeholder={view === 'browse' ? 'Search catalog…' : 'Search installed…'} />
				</div>
			</div>

			{view === 'installed' ? <InstalledView query={query} actions={actions} /> : <BrowseView query={query} actions={actions} />}
		</>
	);
};
