import { useQuery } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { ArrowUpRight, Plug, Plus, ScrollText, Star, Trash2, Wrench } from 'lucide-react';
import { api, errorText, type PluginInfo, type PluginSource } from '../lib/ipc';
import { confirm } from '../lib/dialog';
import { useSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import { Button, cn, Empty, PageHeader, Panel, Select, Switch } from '../components/ui';

const SOURCE_OPTIONS: { value: PluginSource; label: string }[] = [
	{ value: 'Recommended', label: 'Recommended' },
	{ value: 'Latest', label: 'Latest (GitHub)' }
];

/** [name, mod URL, companion addon URL] — same list as v2.6. */
const CREDITS: [string, string | null, string | null][] = [
	['VanillaFixes', 'https://github.com/hannesmann/vanillafixes', null],
	['VanillaHelpers', 'https://github.com/isfir/VanillaHelpers', null],
	['PerfBoost', 'https://gitea.com/avitasia/perf_boost', 'https://gitea.com/avitasia/PerfBoostSettings'],
	['UnitXP_SP3', 'https://codeberg.org/konaka/UnitXP_SP3', 'https://codeberg.org/konaka/UnitXP_SP3_Addon'],
	['Nampower', 'https://gitea.com/avitasia/nampower', 'https://gitea.com/avitasia/NampowerSettings'],
	['SuperWoW', 'https://github.com/balakethelock/SuperWoW', 'https://github.com/balakethelock/SuperAPI'],
	['ClassicAPI', 'https://github.com/brues-code/ClassicAPI', null],
	['Vanilla-Autologin', 'https://github.com/MarcelineVQ/turtle-autologin', null],
	['WeirdUtils Suite', 'https://codeberg.org/MarcelineVQ/WeirdUtils', null],
	['Vanilla Tweaks', 'https://github.com/brndd/vanilla-tweaks', null],
	['DXVK', 'https://github.com/doitsujin/dxvk', null],
	['no1600x1200', null, null]
];

const LinkChip = ({ url, label, tone }: { url: string; label: string; tone: 'accent' | 'success' }) => (
	<button
		onClick={() => openUrl(url)}
		className={cn('inline-flex h-6 items-center gap-0.5 rounded-md bg-white/[0.05] px-2 text-[11px] font-semibold hover:bg-white/[0.1]', tone === 'accent' ? 'text-violet-300' : 'text-green-400')}
	>
		{label}
		<ArrowUpRight size={11} />
	</button>
);

/** Compact toggle row, like v2.6: switch + DLL name, optional source picker on the right. */
const PluginToggle = ({ p }: { p: PluginInfo }) => {
	const { settings, stage } = useSettings();
	const enabled = settings.plugins[p.file] ?? p.defaultEnabled;
	return (
		<div className="flex h-11 items-center gap-3 px-4" title={p.description}>
			<Switch checked={enabled} onChange={v => stage(s => ({ ...s, plugins: { ...s.plugins, [p.file]: v } }))} label={p.file} />
			<span className={cn('min-w-0 flex-1 truncate text-[13px]', enabled ? 'text-fg' : 'text-muted')}>{p.file}</span>
			{p.github && (
				<Select
					value={settings.pluginSources[p.file] ?? 'Recommended'}
					options={SOURCE_OPTIONS}
					onChange={v => stage(s => ({ ...s, pluginSources: { ...s.pluginSources, [p.file]: v } }))}
				/>
			)}
		</div>
	);
};

export const ClientTweaksPage = () => {
	const { settings, stage } = useSettings();
	const catalog = useQuery({ queryKey: ['plugins'], queryFn: api.getPluginCatalog, staleTime: Infinity });
	const core = catalog.data?.filter(p => p.group === 'core') ?? [];
	const optional = catalog.data?.filter(p => p.group === 'optional') ?? [];
	const custom = Object.entries(settings.customPlugins);

	const addCustom = async () => {
		const picked = await open({ multiple: true, title: 'Select Custom DLL', filters: [{ name: 'DLL files', extensions: ['dll', 'DLL'] }] });
		for (const path of picked ? (Array.isArray(picked) ? picked : [picked]) : []) {
			await addCustomDll(path, stage);
		}
	};

	const removeCustom = async (file: string) => {
		const ok = await confirm({
			title: `Remove ${file}?`,
			message: 'This deletes the file from your game folder and removes it from the manager.',
			confirmLabel: 'Remove',
			danger: true
		});
		if (!ok) return;
		try {
			await api.customDllRemove(file);
			stage(s => {
				const next = { ...s.customPlugins };
				delete next[file];
				return { ...s, customPlugins: next };
			});
		} catch (e) {
			toast.error(errorText(e));
		}
	};

	return (
		<>
			<PageHeader title="Client Tweaks" description="Toggle core engine modifications and utilities." />

			<div className="mb-4 grid grid-cols-2 items-start gap-4 [&>section]:mb-0">
				<Panel icon={Star} title="Recommended Core" description="Highly recommended for stability.">
					{core.map(p => (
						<PluginToggle key={p.file} p={p} />
					))}
				</Panel>
				<Panel icon={Wrench} title="Optional WeirdUtils" description="Additional quality-of-life plugins.">
					{optional.map(p => (
						<PluginToggle key={p.file} p={p} />
					))}
				</Panel>
			</div>

			<Panel
				icon={Plug}
				title="Custom User Plugins"
				description="Manage your own third-party DLLs. They will automatically be added/removed from your dlls.txt file."
				actions={
					<Button size="sm" icon={Plus} onClick={addCustom} disabled={!settings.wowDir}>
						Add Custom DLL
					</Button>
				}
			>
				{custom.length === 0 ? (
					<div className="p-4">
						<Empty icon={Plug} title="No custom DLLs managed">
							Click "Add Custom DLL" to import one, or drop a .dll onto the window.
						</Empty>
					</div>
				) : (
					custom.map(([file, on]) => (
						<div key={file} className="group flex h-11 items-center gap-3 px-4">
							<Switch checked={on} onChange={v => stage(s => ({ ...s, customPlugins: { ...s.customPlugins, [file]: v } }))} label={file} />
							<span className="flex-1 truncate text-[13px]">{file}</span>
							<Button
								variant="danger"
								size="sm"
								icon={Trash2}
								title="Remove"
								className="opacity-0 group-hover:opacity-100"
								onClick={() => removeCustom(file)}
							/>
						</div>
					))
				)}
			</Panel>

			<Panel icon={ScrollText} title="Open-Source Credits & Sources" description="This tool packages the work of several open-source developers.">
				<div className="grid grid-cols-3 gap-2 p-4">
					{CREDITS.map(([name, mod, addon]) => (
						<div key={name} className="rounded-lg border border-line bg-white/[0.02] px-3 py-2.5">
							<div className="text-[13px] font-medium">{name}</div>
							<div className="mt-1.5 flex gap-1.5">
								{mod && <LinkChip url={mod} label="Mod" tone="accent" />}
								{addon && <LinkChip url={addon} label="Addon" tone="success" />}
								{!mod && !addon && <span className="text-[11px] text-subtle italic">Legacy engine file</span>}
							</div>
						</div>
					))}
				</div>
			</Panel>
		</>
	);
};

/** Copies a DLL into the game folder and enables it (used by drag & drop too). */
export const addCustomDll = async (path: string, stage: ReturnType<typeof useSettings>['stage']) => {
	try {
		const name = await api.customDllAdd(path);
		stage(s => ({ ...s, customPlugins: { ...s.customPlugins, [name]: true } }));
		toast.success(`Added ${name}. Click Apply Changes to load it.`);
	} catch (e) {
		toast.error(errorText(e));
	}
};
