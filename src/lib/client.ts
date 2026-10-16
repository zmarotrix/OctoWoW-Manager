// Live updater feed from the core: activity log lines and download progress.
import { useSyncExternalStore } from 'react';
import type { ClientLogLine, ClientProgress } from './ipc';

const MAX_LINES = 300;

type Line = ClientLogLine & { id: number; time: string };

let lines: Line[] = [];
let progress: ClientProgress | null = null;
let nextId = 1;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach(l => l());

export const pushClientLog = (l: ClientLogLine) => {
	const time = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
	lines = [...lines.slice(-(MAX_LINES - 1)), { ...l, id: nextId++, time }];
	notify();
};

export const setClientProgress = (p: ClientProgress | null) => {
	progress = p;
	notify();
};

const subscribe = (l: () => void) => {
	listeners.add(l);
	return () => listeners.delete(l);
};

export const useClientLog = () => useSyncExternalStore(subscribe, () => lines);
export const useClientProgress = () => useSyncExternalStore(subscribe, () => progress);
