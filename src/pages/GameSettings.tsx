import { useEffect, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';
import { CircleAlert, CircleCheck, FileInput, FolderOpen, Gauge, Monitor, Terminal } from 'lucide-react';
import { api, errorText, type GpuType, type Tweaks } from '../lib/ipc';
import { toast } from '../lib/toast';
import { confirm } from '../lib/dialog';
import { useSettings } from '../lib/settings';
import { Badge, Button, cn, Input, PageHeader, Panel, Row, Segmented, Select, Slider, Switch } from '../components/ui';

const screenW = Math.round(window.screen.width * window.devicePixelRatio);
const screenH = Math.round(window.screen.height * window.devicePixelRatio);

const RATIOS: { value: string; label: string; ratio: number }[] = [
	{ value: '', label: screenW ? `Auto (${screenW}×${screenH})` : 'Auto', ratio: screenW / screenH || 16 / 9 },
	{ value: '4:3', label: '4:3 Standard', ratio: 4 / 3 },
	{ value: '16:9', label: '16:9 Widescreen', ratio: 16 / 9 },
	{ value: '16:10', label: '16:10 Widescreen', ratio: 16 / 10 },
	{ value: '21:9', label: '21:9 Ultrawide', ratio: 21 / 9 },
	{ value: '32:9', label: '32:9 Super Ultrawide', ratio: 32 / 9 }
];

/** Horizontal FoV scaled from the stock 4:3 / 90° view. */
const fovFor = (aspect: string) => {
	const ratio = RATIOS.find(r => r.value === aspect)?.ratio ?? 4 / 3;
	return Number((2 * Math.atan((ratio / (4 / 3)) * Math.tan(1.570796 / 2))).toFixed(4));
};

type NumKey = 'farclip' | 'frill' | 'nameplate' | 'maxCamera' | 'soundChannels';
const SLIDERS: { key: NumKey; label: string; min: number; safe: number; extreme: number; tip: string }[] = [
	{ key: 'farclip', label: 'Render Distance', min: 777, safe: 1500, extreme: 10000, tip: 'Maximum terrain render distance.' },
	{ key: 'frill', label: 'Ground Clutter', min: 70, safe: 300, extreme: 1000, tip: 'How far grass and ground clutter are drawn.' },
	{ key: 'nameplate', label: 'Nameplate Distance', min: 20, safe: 41, extreme: 150, tip: 'Distance at which enemy nameplates appear.' },
	{ key: 'maxCamera', label: 'Max Camera Zoom', min: 50, safe: 100, extreme: 250, tip: 'How far the camera can zoom out.' },
	{ key: 'soundChannels', label: 'Audio Channels', min: 12, safe: 64, extreme: 128, tip: 'Simultaneous sounds the game can play.' }
];

type BoolKey = 'quickloot' | 'backgroundSound' | 'largeAddressAware' | 'cameraSkipFix' | 'corruptInterfaceBypass' | 'disableDep';
const QOL: { key: BoolKey; label: string; tip: string }[] = [
	{ key: 'quickloot', label: 'Always Auto-Loot', tip: 'Reverses the auto-loot modifier so you always auto-loot.' },
	{ key: 'backgroundSound', label: 'Background Sounds', tip: 'Keep game audio playing while the window is in the background.' },
	{ key: 'largeAddressAware', label: 'Large Address Aware (4GB)', tip: 'Let the client use up to 4 GB of RAM.' },
	{ key: 'cameraSkipFix', label: 'Fix Camera Skip', tip: 'Stops the view snapping when you rotate the camera.' },
	{ key: 'corruptInterfaceBypass', label: 'Corrupt Interface Bypass', tip: "Skips GlueXML signature checks that cause 'Corrupt Interface' errors." },
	{ key: 'disableDep', label: 'Disable DEP', tip: 'Builds WoW_Tweaked.exe without opting into Data Execution Prevention. No admin prompt needed.' }
];

export const usePlatform = () => useQuery({ queryKey: ['platform'], queryFn: api.getPlatform, staleTime: Infinity }).data ?? 'windows';

/** Pull settings from v2.6's octowow_config.json. */
const importV26 = async (replace: (fn: (s: import('../lib/ipc').Settings) => import('../lib/ipc').Settings) => void) => {
	const path = await open({ title: 'Select your v2.6 octowow_config.json', filters: [{ name: 'octowow_config.json', extensions: ['json'] }] });
	if (typeof path !== 'string') return;
	const ok = await confirm({
		title: 'Import v2.6 settings?',
		message: 'Your game folder, tweaks, plugin choices, custom DLLs, mod descriptions and tracked addons will be replaced with the ones from that file.',
		confirmLabel: 'Import'
	});
	if (!ok) return;
	try {
		const r = await api.importLegacyConfig(path);
		replace(() => r.settings);
		toast.success(`${r.summary}. Click Apply Changes to use them.`);
	} catch (e) {
		toast.error(errorText(e));
	}
};

/** Compact switch + label (description as a tooltip), like v2.6's switch grid. */
const Toggle = ({ label, tip, checked, onChange }: { label: string; tip: string; checked: boolean; onChange: (v: boolean) => void }) => (
	<label className="flex h-9 cursor-pointer items-center gap-3 text-[13px]" title={tip}>
		<Switch checked={checked} onChange={onChange} label={label} />
		<span className={checked ? 'text-fg' : 'text-muted'}>{label}</span>
	</label>
);

/** What DXVK can do on this PC, from the core's Vulkan probe. */
const GpuHint = ({ choice }: { choice: GpuType }) => {
	const info = useQuery({ queryKey: ['gpuInfo'], queryFn: api.gpuInfo, staleTime: Infinity });
	const v = info.data;
	if (!v?.checked) return null;
	const d = v.device;
	const [tone, text] = !d
		? ['text-amber-400', "No Vulkan graphics driver found. DXVK can't run here, so Apply uses DirectX 9 (Off). Updating your graphics driver may enable DXVK."]
		: d.major === 1 && d.minor < 1
			? ['text-amber-400', `${d.name} supports Vulkan ${d.major}.${d.minor}, which is too old for DXVK. Apply uses DirectX 9 (Off).`]
			: d.major === 1 && d.minor < 3
				? ['text-amber-400', `${d.name} supports Vulkan ${d.major}.${d.minor}. DXVK 2.x needs 1.3, so choose Older GPUs (Apply switches to it automatically).`]
				: ['text-subtle', `${d.name} · Vulkan ${d.major}.${d.minor}: every DXVK option works.${d.vendorId === 0x1002 && choice === 'Standard' ? ' AMD GPU detected: try AMD Specific DXVK if you get crashes.' : ''}`];
	return <p className={cn('mt-2 text-xs', tone)}>{text}</p>;
};

const InstallPanel = () => {
	const { settings, update } = useSettings();
	const platform = usePlatform();
	const [draft, setDraft] = useState(settings.wowDir);
	useEffect(() => setDraft(settings.wowDir), [settings.wowDir]);

	const info = useQuery({
		queryKey: ['client', settings.wowDir],
		queryFn: () => api.validateGameDir(settings.wowDir),
		enabled: !!settings.wowDir
	});

	const commit = (dir: string) => update(s => ({ ...s, wowDir: dir.trim() }));
	const browse = async () => {
		const dir = await open({ directory: true, defaultPath: settings.wowDir || undefined });
		if (typeof dir === 'string') commit(dir);
	};

	return (
		<Panel
			icon={FolderOpen}
			title="Installation Directory"
			actions={
				<Button size="sm" variant="ghost" icon={FileInput} onClick={() => importV26(update)} title="Import settings from v2.6's octowow_config.json">
					Import v2.6 settings
				</Button>
			}
		>
			<div className="flex gap-2 px-5 py-4">
				<Input
					className="flex-1 font-mono"
					value={draft}
					onChange={e => setDraft(e.target.value)}
					onBlur={() => commit(draft)}
					onKeyDown={e => e.key === 'Enter' && commit(draft)}
					placeholder="C:\Games\OctoWoW"
				/>
				<Button onClick={browse}>Browse…</Button>
			</div>
			{settings.wowDir && info.data && (
				<div className="flex items-center gap-2 px-5 py-3 text-xs">
					{info.data.valid ? <CircleCheck size={14} className="text-success" /> : <CircleAlert size={14} className="text-warning" />}
					<span className={info.data.valid ? 'text-muted' : 'text-amber-400'}>{info.data.message}</span>
					{info.data.valid && (
						<span className="ml-auto">
							{info.data.hasTweakedExe ? <Badge tone="success">WoW_Tweaked.exe</Badge> : <Badge tone="warning">Not patched</Badge>}
						</span>
					)}
				</div>
			)}
			{platform !== 'windows' && (
				<Row
					label={
						<span className="flex items-center gap-2">
							<Terminal size={14} className="text-subtle" />
							Wine command
						</span>
					}
					description="How to run the Windows client: e.g. wine, a Proton wrapper, or a CrossOver/Bottles command. Leave empty for wine."
				>
					<Input className="w-64 font-mono" value={settings.launchCommand} onChange={e => update(s => ({ ...s, launchCommand: e.target.value }))} placeholder="wine" />
				</Row>
			)}
		</Panel>
	);
};

export const GameSettingsPage = () => {
	const { settings, stage, update } = useSettings();
	const platform = usePlatform();
	const t = settings.tweaks;
	const setTweaks = (patch: Partial<Tweaks>) => stage(s => ({ ...s, tweaks: { ...s.tweaks, ...patch } }));

	// First run: derive FoV from the detected aspect ratio, as v2.6 did.
	useEffect(() => {
		if (t.fov === 0) update(s => ({ ...s, tweaks: { ...s.tweaks, fov: fovFor(s.tweaks.aspectRatio) } }));
	}, [t.fov, update]);

	const toggleSafety = (override: boolean) =>
		stage(s => {
			if (override) return { ...s, safetyOverride: true };
			const tweaks = { ...s.tweaks };
			for (const sl of SLIDERS) tweaks[sl.key] = Math.min(tweaks[sl.key], sl.safe);
			return { ...s, safetyOverride: false, tweaks };
		});

	return (
		<>
			<PageHeader title="Game Settings" description="Client location, graphics and engine limits. Changes are written when you click Apply." />
			<InstallPanel />

			<Panel icon={Monitor} title="Environment & Quality of Life">
				<div className="px-5 py-4">
					<Segmented
						value={settings.gpuType}
						options={[
							{ value: 'Standard', label: 'Standard DXVK (NVIDIA/Intel)' },
							{ value: 'AMD', label: 'AMD Specific DXVK' },
							{ value: 'Legacy', label: 'Older GPUs (DXVK 1.10)' },
							{ value: 'Off', label: 'Off (DirectX 9)' }
						]}
						onChange={g => stage(s => ({ ...s, gpuType: g }))}
					/>
					<GpuHint choice={settings.gpuType} />
					<div className="mt-4 grid grid-cols-2 gap-x-10 gap-y-1">
						<Toggle
							label="Auto Login Mod"
							tip="Automates the authentication process. Bypasses the standard login screen."
							checked={settings.installAutologin}
							onChange={v => stage(s => ({ ...s, installAutologin: v }))}
						/>
						{QOL.filter(q => q.key !== 'disableDep' || platform === 'windows').map(q => (
							<Toggle key={q.key} label={q.label} tip={q.tip} checked={t[q.key]} onChange={v => setTweaks({ [q.key]: v })} />
						))}
					</div>
				</div>
			</Panel>

			<Panel icon={Gauge} title="Engine Adjustments">
				<div className="flex flex-wrap items-center gap-3 px-5 pt-4 pb-2 text-[13px]">
					<span className="text-muted">Aspect Ratio</span>
					<Select value={t.aspectRatio} options={RATIOS} onChange={v => setTweaks({ aspectRatio: v, fov: fovFor(v) })} />
					<span className="ml-3 text-muted" title="Horizontal field of view in radians, calculated from the aspect ratio.">
						FoV (Rad)
					</span>
					<input
						type="number"
						step={0.0001}
						value={t.fov}
						onChange={e => setTweaks({ fov: Number(e.target.value) })}
						className="h-8 w-24 rounded-lg border border-line-strong bg-black/30 px-3 text-right font-mono text-[13px] outline-none focus:border-accent"
					/>
					<label className="ml-auto flex items-center gap-2.5 text-xs text-muted" title="Warning: Exceeding max limits may cause game instability.">
						Disable Safety Limits
						<Switch tone="danger" checked={settings.safetyOverride} onChange={toggleSafety} label="Disable Safety Limits" />
					</label>
				</div>
				<div className="px-5 pb-3">
					{SLIDERS.map(sl => (
						<div key={sl.key} className="flex items-center gap-4 py-2" title={sl.tip}>
							<span className="w-40 shrink-0 text-[13px]">{sl.label}</span>
							<div className="flex flex-1 justify-end [&>div]:w-full">
								<Slider
									min={sl.min}
									max={settings.safetyOverride ? sl.extreme : sl.safe}
									value={t[sl.key]}
									danger={settings.safetyOverride && t[sl.key] > sl.safe}
									onChange={v => setTweaks({ [sl.key]: v })}
								/>
							</div>
						</div>
					))}
				</div>
			</Panel>
		</>
	);
};
