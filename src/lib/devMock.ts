// Fake backend for previewing the UI in a plain browser (`npm run dev`).
// Only loaded in dev when not running inside Tauri; never bundled into the app.
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import type { AddonEntry, CatalogEntry, ClientPlan, FileReport, MpqEntry, Settings } from './ipc';

let settings: Settings = {
	wowDir: 'G:\\Games\\WoW Private Server\\OctoWoW',
	gpuType: 'Standard',
	installAutologin: true,
	safetyOverride: false,
	tweaks: {
		aspectRatio: '',
		fov: 2.1235,
		farclip: 1500,
		frill: 300,
		nameplate: 41,
		maxCamera: 100,
		soundChannels: 64,
		quickloot: true,
		backgroundSound: true,
		largeAddressAware: true,
		cameraSkipFix: true,
		corruptInterfaceBypass: true,
		disableDep: false
	},
	plugins: {},
	pluginSources: { 'SuperWoWhook.dll': 'Latest' },
	customPlugins: { 'MyCustomHook.dll': true },
	gameModsMeta: {},
	trackedAddons: [],
	ignoredAddons: [],
	githubToken: '',
	launchCommand: '',
	pendingApply: false
};

const addon = (folder: string, title: string, version: string, author: string, notes: string, extra: Partial<AddonEntry> = {}): AddonEntry => ({
	folder, title, version, author, notes, url: null, localSha: 'a1', remoteSha: null, updateAvailable: false, managed: false, missing: false, isGit: false, ...extra
});

const gh = (repo: string) => ({ url: `https://github.com/${repo}`, managed: true });

const ADDONS: AddonEntry[] = [
	addon('AtlasLoot', 'AtlasLoot', '1.12.1', 'shaga', 'Loot browser for every dungeon and raid', { ...gh('shaga/AtlasLoot'), isGit: true }),
	addon('aux-addon', 'aux', '', 'shirsig', 'Auction House replacement with advanced filtering and search', gh('shirsig/aux-addon-vanilla')),
	addon('Bagnon', 'Bagnon', '1.0.4', 'Tuller', 'Single-window inventory display'),
	addon('BigWigs', 'BigWigs', 'r20000', 'BigWigs Team', 'Boss mod with timers and warnings'),
	addon('pfQuest', 'pfQuest', '7.0.1', 'shagu', 'Quest helper and database browser', { ...gh('shagu/pfQuest'), updateAvailable: true, remoteSha: 'b2' }),
	addon('pfUI', 'pfUI', '5.4.3', 'shagu', 'A full UI replacement for the 1.12 client', gh('shagu/pfUI')),
	addon('ShaguTweaks', 'ShaguTweaks', '1.9.0', 'shagu', 'Quality of life improvements without changing the look', { ...gh('shagu/ShaguTweaks'), updateAvailable: true, remoteSha: 'c3' }),
	addon('SuperAPI', 'SuperAPI', '2.2', 'balake', 'Companion addon for SuperWoW'),
	addon('WIM', 'WIM (WoW Instant Messenger)', '1.3.2', 'Pazza', 'Instant-messenger style whispers'),
	addon('Questie', 'Questie', '', '', 'Managed by OctoWoW, but the files are missing from your drive.', { ...gh('AeroScripts/QuestieDev'), missing: true })
];

const MODS: MpqEntry[] = [
	{ baseName: 'patch-Y.mpq', fileName: 'patch-Y.mpq', enabled: true, size: 734_003_200, title: 'HD Character Models', description: 'Modern high-resolution player and NPC models.' },
	{ baseName: 'patch-Z.mpq', fileName: 'patch-Z.mpq', enabled: true, size: 128_450_560, title: 'HD Water & Sky', description: 'Higher resolution water and skybox textures.' },
	{ baseName: 'patch-L.mpq', fileName: 'patch-L.mpq.disabled', enabled: false, size: 12_582_912, title: 'patch-L.mpq', description: '' }
];

const file = (path: string, size: number, extra: Partial<FileReport> = {}): FileReport => ({
	path, size, localSize: size, state: 'ok', detail: '', download: false, isNew: false, ...extra
});

let plan: ClientPlan | null = null;
const PLAN: ClientPlan = {
	status: 'updateAvailable',
	checkedAt: Date.now() / 1000,
	release: { infoHash: '9f2c41d07be35a6c', files: 50, totalSize: 9_452_840_960, created: null, published: 'Tue, 13 Oct 2026 02:39:35 GMT', webSeed: 'https://dl.octowow.st/client/' },
	baseline: { wowDir: settings.wowDir, infoHash: '51ab02ee9c', syncedAt: Date.now() / 1000 - 86400 * 9, verified: false },
	releaseChanged: true,
	deepVerified: false,
	sizeOnly: false,
	files: [
		file('Data/patch-3.mpq', 2_060_279_959, { state: 'outdated', download: true, localSize: 1_998_100_224, detail: 'Updated in this release (1.9 GB → 1.9 GB)' }),
		file('Data/patch-5.mpq', 54_360_624, { state: 'missing', download: true, localSize: null, isNew: true, detail: 'Not on disk' }),
		file('Data/patch-4.mpq', 1_368_080_007, { state: 'outdated', download: true, detail: 'Changed in this release (same size)' }),
		file('WoW.exe', 7_024_640, { state: 'corrupt', download: true, detail: "Contents don't match the release (corrupted or modified)" }),
		file('d3d9.dll', 4_501_518, { state: 'managed', localSize: 4_812_288, detail: 'Managed by Client Tweaks; never overwritten' }),
		file('nampower.dll', 829_952, { state: 'managed', detail: 'Managed by Client Tweaks; never overwritten' }),
		file('realmlist.wtf', 32, { state: 'userEdited', localSize: 41, detail: 'Your edits are kept' })
	],
	downloadFiles: 4,
	downloadBytes: 3_489_745_230,
	obsolete: [{ path: 'Data/patch-6.mpq', size: 451_195_806 }],
	dropped: ['Data/patch-B.mpq']
};

const LOG: [string, string][] = [
	['info', 'Release 9f2c41d0 · 50 files · 8.8 GB'],
	['info', 'A new release is out since your last sync'],
	['info', 'Hash-checking 1 file(s) (1.0 MB)…'],
	['warn', 'Data/patch-3.mpq: Updated in this release (1.9 GB → 1.9 GB)'],
	['warn', 'Data/patch-4.mpq: Changed in this release (same size)'],
	['warn', '4 file(s) to download · 3.3 GB']
];

const cat = (name: string, owner: string, description: string, stars: number, git = `https://github.com/${owner}/${name}.git`): CatalogEntry => ({
	name, owner, git, title: name, description, version: '1.0', author: owner, lastUpdated: '2026-09-01T00:00:00Z', stars
});
const CATALOG: CatalogEntry[] = [
	cat('AtlasLoot', 'shaga', 'Loot browser for every dungeon and raid, including the custom OctoWoW instances', 12, 'https://github.com/shaga/AtlasLoot.git'),
	cat('Bagshui', 'absir', 'Inventory addon with automatic sorting, categories and an item catalog', 48),
	cat('BigWigs', 'pepopo978', 'Boss mod with timers and warnings for every raid encounter', 31),
	cat('DoiteAuras', 'Player-Doite', 'Lightweight aura and cooldown tracker', 6),
	cat('NotGrid', 'gnwl', 'Compact raid frames with healing and range indicators', 22),
	cat('pfQuest', 'shagu', 'Quest helper and database browser', 210, 'https://github.com/shagu/pfQuest.git'),
	cat('Questie', 'AeroScripts', 'Shows quests on the map and minimap', 98, 'https://github.com/AeroScripts/QuestieDev.git')
];

const sleep = (ms: number) => new Promise(r => setTimeout(r, ms));
let nextJob = 1;

const fakeJob = (label: string, steps: string[], fail?: string) => {
	const id = nextJob++;
	(async () => {
		for (let i = 0; i < steps.length; i++) {
			await emit('job', { id, label, status: 'running', done: i, total: steps.length, message: steps[i] });
			await sleep(400);
		}
		await emit('job', fail
			? { id, label, status: 'failed', done: 0, total: 0, message: fail }
			: { id, label, status: 'finished', done: 1, total: 1, message: 'Done' });
	})();
	return id;
};

mockWindows('main');
mockIPC(
	async (cmd, args) => {
		switch (cmd) {
			case 'get_settings':
				return settings;
			case 'save_settings':
				settings = (args as { settings: Settings }).settings;
				return;
			case 'validate_game_dir':
				return { valid: true, hasWowExe: true, hasTweakedExe: true, hasDataDir: true, hasAddonsDir: true, message: 'Valid 1.12 client.' };
			case 'get_plugin_catalog':
				return (await import('./devCatalog')).CATALOG;
			case 'scan_game_mods':
				await sleep(80);
				return MODS;
			case 'scan_addons':
				await sleep(120);
				return ADDONS;
			case 'client_status':
				return plan;
			case 'client_check': {
				const id = fakeJob('Checking for game updates', ['Fetching release info', 'Hash-checking files']);
				(async () => {
					for (const [level, text] of LOG) {
						await sleep(120);
						await emit('client-log', { level, text });
					}
					plan = PLAN;
					await emit('state-changed', 'clientUpdate');
				})();
				return id;
			}
			case 'client_verify':
				return fakeJob('Verifying game files', ['Hash-checking files · 1.2 GB of 8.8 GB', 'Hash-checking files · 6.0 GB of 8.8 GB']);
			case 'client_update': {
				const id = nextJob++;
				(async () => {
					const sel = PLAN.files.filter(f => f.download);
					const total = sel.reduce((a, f) => a + f.size, 0);
					for (let t = 0; t <= 20; t++) {
						const files = sel.map((f, i) => ({ path: f.path, size: f.size, done: Math.min(f.size, Math.round(f.size * Math.max(0, t / 12 - i * 0.2))) }));
						const done = files.reduce((a, f) => a + f.done, 0);
						await emit('client-progress', { phase: t ? 'downloading' : 'connecting', bytesDone: done, bytesTotal: total, speed: t ? 48_000_000 : 0, connections: 6, files });
						await emit('job', { id, label: 'Updating game files', status: 'running', done, total, message: 'Downloading' });
						await sleep(350);
					}
				})();
				return id;
			}
			case 'client_cancel':
				return;
			case 'apply_changes':
				return fakeJob('Applying changes', ['Copying plugins', 'Writing dlls.txt', 'Patching WoW.exe'], 'Apply pipeline not implemented yet');
			case 'get_platform':
				return 'windows';
			case 'mpq_set_enabled':
			case 'mpq_delete':
			case 'custom_dll_remove':
			case 'addon_remove':
				await sleep(60);
				return;
			case 'mpq_install':
			case 'custom_dll_add':
				return String((args as { path: string }).path).split(/[\\/]/).pop();
			case 'addon_install': {
				await sleep(900);
				const url = (args as { url: string }).url;
				return { folders: [url.replace(/\.git$/, '').split('/').pop()], url };
			}
			case 'addon_install_zip':
				await sleep(500);
				return ['FromZip'];
			case 'addon_catalog':
				await sleep(300);
				return CATALOG;
			case 'check_addon_updates':
				return fakeJob('Checking addons for updates', ['Asking repositories for their latest commit']);
			case 'gpu_info':
				return { checked: true, device: { name: 'NVIDIA GeForce GTX 750 Ti', vendorId: 0x10de, major: 1, minor: 2 } };
			case 'check_app_update':
				return { current: '3.0.0', latest: 'v3.1.0', url: 'https://github.com/zmarotrix/OctoWoW-Manager/releases', newer: true };
			case 'launch_game':
				throw 'Launching is not implemented yet';
			default:
				return undefined;
		}
	},
	{ shouldMockEvents: true }
);
