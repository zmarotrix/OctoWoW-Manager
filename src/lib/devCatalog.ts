// Mirror of src-tauri/src/core/plugins.rs for the browser preview only.
import type { PluginInfo } from './ipc';

const core = (file: string, description: string, extra: Partial<PluginInfo> = {}): PluginInfo => ({
	file, group: 'core', description, defaultEnabled: true, github: null, companionAddon: null, ...extra
});
const optional = (file: string, description: string): PluginInfo => ({
	file, group: 'optional', description, defaultEnabled: false, github: null, companionAddon: null
});

export const CATALOG: PluginInfo[] = [
	core('ClassicAPI.dll', 'Backports a massive part of the modern WoW API (550+ functions, Lua 5.1 syntax).', { github: 'brues-code/ClassicAPI' }),
	core('nampower.dll', 'Implements ping compensation. Bypasses a 1.12 client flaw to enable queueing spell casts.'),
	core('no1600x1200.dll', 'Removes the hardcoded 1600x1200 resolution limit.'),
	core('perf_boost.dll', 'Provides dynamic render distance controls (culling).'),
	core('SuperWoWhook.dll', 'Injects a massively expanded Lua API. Increases macro limit.', { github: 'balakethelock/SuperWoW' }),
	core('transmogfix.dll', 'Eliminates FPS drops caused by rapid equipment visual updates.'),
	core('UnitXP_SP3.dll', 'Engine-level optimizations replacing legacy assembly.'),
	core('VanillaHelpers.dll', 'Expands engine limits. Raises the client memory allocator from 2GB to 4GB.', { github: 'isfir/VanillaHelpers' }),
	core('weirdperformance.dll', 'Engine-level optimizations: SIMD math replacements, etc.'),
	optional('bigcursor.dll', 'Upscales the hardware cursor for improved visibility on modern resolutions.'),
	optional('customassets.dll', 'Enables loading loose game asset files from the Data/ directory.'),
	optional('logsessions.dll', 'Organizes combat and chat logs into clean, per-character, per-day files automatically.'),
	optional('minimapicons.dll', 'Adds TBC/WotLK-style minimap tracking icons for NPCs and objects.'),
	optional('pngscreenshots.dll', 'Saves screenshots as compressed PNG files to completely eliminate frame drops.'),
	optional('worldmarkers.dll', 'Place up to 5 animated colored markers (Cataclysm style) in the world.')
];
