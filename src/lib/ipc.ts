// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
import { invoke } from '@tauri-apps/api/core';

export type GpuType = 'Standard' | 'AMD' | 'Legacy' | 'Off';

export type VulkanStatus = { checked: boolean; device: { name: string; vendorId: number; major: number; minor: number } | null };
export type PluginSource = 'Recommended' | 'Latest';

export type Tweaks = {
	aspectRatio: string;
	fov: number;
	farclip: number;
	frill: number;
	nameplate: number;
	maxCamera: number;
	soundChannels: number;
	quickloot: boolean;
	backgroundSound: boolean;
	largeAddressAware: boolean;
	cameraSkipFix: boolean;
	corruptInterfaceBypass: boolean;
	disableDep: boolean;
};

export type GameModMeta = { title: string; description: string };

export type Settings = {
	wowDir: string;
	gpuType: GpuType;
	installAutologin: boolean;
	safetyOverride: boolean;
	tweaks: Tweaks;
	plugins: Record<string, boolean>;
	pluginSources: Record<string, PluginSource>;
	customPlugins: Record<string, boolean>;
	gameModsMeta: Record<string, GameModMeta>;
	trackedAddons: string[];
	ignoredAddons: string[];
	githubToken: string;
	launchCommand: string;
	pendingApply: boolean;
};

export type ClientInfo = {
	valid: boolean;
	hasWowExe: boolean;
	hasTweakedExe: boolean;
	hasDataDir: boolean;
	hasAddonsDir: boolean;
	message: string;
};

export type PluginInfo = {
	file: string;
	group: 'core' | 'optional';
	description: string;
	defaultEnabled: boolean;
	github: string | null;
	companionAddon: string | null;
};

export type MpqEntry = {
	baseName: string;
	fileName: string;
	enabled: boolean;
	size: number;
	title: string;
	description: string;
};

export type AddonEntry = {
	folder: string;
	title: string;
	version: string;
	author: string;
	notes: string;
	url: string | null;
	localSha: string | null;
	remoteSha: string | null;
	updateAvailable: boolean;
	managed: boolean;
	missing: boolean;
	isGit: boolean;
};

export type CatalogEntry = {
	name: string;
	owner: string;
	git: string;
	title: string;
	description: string;
	version: string;
	author: string;
	lastUpdated: string | null;
	stars: number | null;
};

export type AppUpdate = { current: string; latest: string; url: string; newer: boolean };

export type JobId = number;

export type FileState = 'ok' | 'missing' | 'sizeMismatch' | 'outdated' | 'corrupt' | 'managed' | 'userEdited';

export type FileReport = {
	path: string;
	size: number;
	localSize: number | null;
	state: FileState;
	detail: string;
	download: boolean;
	isNew: boolean;
};

export type ClientPlan = {
	status: 'notInstalled' | 'upToDate' | 'updateAvailable' | 'repairNeeded';
	checkedAt: number;
	release: { infoHash: string; files: number; totalSize: number; created: number | null; published: string | null; webSeed: string | null };
	baseline: { wowDir: string; infoHash: string; syncedAt: number; verified: boolean } | null;
	releaseChanged: boolean;
	deepVerified: boolean;
	sizeOnly: boolean;
	files: FileReport[];
	downloadFiles: number;
	downloadBytes: number;
	obsolete: { path: string; size: number }[];
	dropped: string[];
};

export type ClientLogLine = { level: 'info' | 'warn' | 'error' | 'ok'; text: string };

export type ClientProgress = {
	phase: 'connecting' | 'downloading';
	bytesDone: number;
	bytesTotal: number;
	speed: number;
	connections: number;
	files: { path: string; size: number; done: number }[];
};

export const api = {
	getSettings: () => invoke<Settings>('get_settings'),
	saveSettings: (settings: Settings) => invoke<void>('save_settings', { settings }),
	validateGameDir: (path: string) => invoke<ClientInfo>('validate_game_dir', { path }),
	getPluginCatalog: () => invoke<PluginInfo[]>('get_plugin_catalog'),
	scanGameMods: () => invoke<MpqEntry[]>('scan_game_mods'),
	getPlatform: () => invoke<'windows' | 'linux' | 'macos'>('get_platform'),
	mpqSetEnabled: (baseName: string, enabled: boolean) => invoke<void>('mpq_set_enabled', { baseName, enabled }),
	mpqDelete: (fileName: string) => invoke<void>('mpq_delete', { fileName }),
	mpqInstall: (path: string) => invoke<string>('mpq_install', { path }),
	customDllAdd: (path: string) => invoke<string>('custom_dll_add', { path }),
	customDllRemove: (file: string) => invoke<void>('custom_dll_remove', { file }),
	scanAddons: () => invoke<AddonEntry[]>('scan_addons'),
	checkAddonUpdates: () => invoke<JobId>('check_addon_updates'),
	addonInstall: (url: string) => invoke<{ folders: string[]; url: string }>('addon_install', { url }),
	addonInstallZip: (path: string) => invoke<string[]>('addon_install_zip', { path }),
	addonRemove: (folder: string) => invoke<void>('addon_remove', { folder }),
	addonCatalog: (refresh = false) => invoke<CatalogEntry[]>('addon_catalog', { refresh }),
	checkAppUpdate: () => invoke<AppUpdate>('check_app_update'),
	gpuInfo: () => invoke<VulkanStatus>('gpu_info'),
	importLegacyConfig: (path: string) => invoke<{ settings: Settings; summary: string }>('import_legacy_config', { path }),
	clientStatus: () => invoke<ClientPlan | null>('client_status'),
	clientCheck: () => invoke<JobId>('client_check'),
	clientVerify: () => invoke<JobId>('client_verify'),
	clientUpdate: () => invoke<JobId>('client_update'),
	clientCancel: () => invoke<void>('client_cancel'),
	applyChanges: () => invoke<JobId>('apply_changes'),
	launchGame: () => invoke<void>('launch_game')
};

/** Commands reject with a plain string; normalize for display. */
export const errorText = (e: unknown) => (typeof e === 'string' ? e : e instanceof Error ? e.message : String(e));

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];
export const formatBytes = (n: number) => {
	let v = n;
	let u = 0;
	while (v >= 1024 && u < UNITS.length - 1) {
		v /= 1024;
		u++;
	}
	return u === 0 ? `${n} B` : `${v.toFixed(1)} ${UNITS[u]}`;
};

/** Same normalisation as the core's `url_key`, for matching repo URLs. */
export const repoKey = (url: string) =>
	url
		.trim()
		.replace(/[?#].*$/, '')
		.replace(/\/(tree|-\/tree|src\/branch)\/.*$/, '')
		.replace(/\/+$/, '')
		.replace(/\.git$/, '')
		.toLowerCase()
		// Both OctoWoW git addresses name the same repos.
		.replace(/^https:\/\/git\.octowow\.st\/git\//, 'https://octowow.st/git/');
