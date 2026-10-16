import { useEffect, useSyncExternalStore } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { QueryClient } from '@tanstack/react-query';
import { pushClientLog, setClientProgress } from './client';
import type { ClientLogLine, ClientProgress } from './ipc';

export type JobEvent = {
	id: number;
	label: string;
	status: 'running' | 'finished' | 'failed';
	done: number;
	total: number;
	message: string;
};

const LINGER_MS = { finished: 4000, failed: 8000 };

let jobs: JobEvent[] = [];
const listeners = new Set<() => void>();
const notify = () => listeners.forEach(l => l());

const upsert = (ev: JobEvent) => {
	jobs = jobs.some(j => j.id === ev.id) ? jobs.map(j => (j.id === ev.id ? ev : j)) : [...jobs, ev];
	notify();
	if (ev.status !== 'running') {
		window.setTimeout(() => {
			jobs = jobs.filter(j => j.id !== ev.id);
			notify();
		}, LINGER_MS[ev.status]);
	}
};

const subscribe = (l: () => void) => {
	listeners.add(l);
	return () => listeners.delete(l);
};

export const useJobs = () => useSyncExternalStore(subscribe, () => jobs);

/** Wires core events into the job store and the query cache. Mount once. */
export const useCoreEvents = (queryClient: QueryClient) => {
	useEffect(() => {
		const unlisten = [
			listen<JobEvent>('job', e => {
				upsert(e.payload);
				if (e.payload.status !== 'running') setClientProgress(null);
			}),
			listen<ClientLogLine>('client-log', e => pushClientLog(e.payload)),
			listen<ClientProgress>('client-progress', e => setClientProgress(e.payload)),
			listen<string>('state-changed', e => queryClient.invalidateQueries({ queryKey: [e.payload] }))
		];
		return () => unlisten.forEach(p => p.then(f => f()));
	}, [queryClient]);
};
