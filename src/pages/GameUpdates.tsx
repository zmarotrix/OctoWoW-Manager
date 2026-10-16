import { useEffect, useMemo, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { ChevronRight, CloudDownload, RefreshCw, ShieldCheck, Square, Terminal } from 'lucide-react';
import { api, errorText, formatBytes, type ClientPlan, type FileReport } from '../lib/ipc';
import { useClientLog, useClientProgress } from '../lib/client';
import { useJobs } from '../lib/jobs';
import { useSettings } from '../lib/settings';
import { toast } from '../lib/toast';
import { Badge, Button, cn, Panel } from '../components/ui';

const CLIENT_JOBS = ['Checking for game updates', 'Verifying game files', 'Updating game files'];

const ago = (unix: number) => {
	const s = Date.now() / 1000 - unix;
	if (s < 90) return 'just now';
	if (s < 3600 * 1.5) return `${Math.round(s / 60)} min ago`;
	if (s < 86400 * 1.5) return `${Math.round(s / 3600)} h ago`;
	return `${Math.round(s / 86400)} days ago`;
};

const duration = (sec: number) =>
	!isFinite(sec) || sec <= 0 ? '—' : sec < 60 ? `${Math.ceil(sec)}s` : sec < 3600 ? `${Math.floor(sec / 60)}m ${Math.round(sec % 60)}s` : `${Math.floor(sec / 3600)}h ${Math.round((sec % 3600) / 60)}m`;

const STATE_BADGE: Record<FileReport['state'], [string, 'accent' | 'warning' | 'danger' | 'neutral' | 'success']> = {
	ok: ['OK', 'success'],
	outdated: ['Updated', 'accent'],
	missing: ['Missing', 'warning'],
	sizeMismatch: ['Incomplete', 'warning'],
	corrupt: ['Corrupt', 'danger'],
	managed: ['Client Tweaks', 'neutral'],
	userEdited: ['Your edits', 'neutral']
};

const splitPath = (p: string) => {
	const i = p.lastIndexOf('/');
	return i < 0 ? ['', p] : [p.slice(0, i + 1), p.slice(i + 1)];
};

const FileRow = ({ f, done }: { f: FileReport; done?: number }) => {
	const [dir, name] = splitPath(f.path);
	const [label, tone] = STATE_BADGE[f.state];
	const pct = done !== undefined && f.size > 0 ? Math.min(100, (done / f.size) * 100) : null;
	return (
		<div className="lazy-row px-4 py-2.5">
			<div className="flex items-center gap-3">
				<div className="min-w-0 flex-1">
					<div className="truncate text-[13px]">
						<span className="text-subtle">{dir}</span>
						<span className="font-medium">{name}</span>
					</div>
					<div className="truncate text-xs text-subtle">{f.detail}</div>
				</div>
				{f.isNew && <Badge tone="success">New</Badge>}
				<Badge tone={tone}>{label}</Badge>
				<span className="w-20 text-right font-mono text-[11px] text-muted tabular-nums">{formatBytes(f.size)}</span>
			</div>
			{pct !== null && (
				<div className="mt-2 flex items-center gap-2">
					<div className="h-1 flex-1 overflow-hidden rounded-full bg-white/[0.06]">
						<div className="brand-gradient h-full rounded-full transition-[width] duration-300" style={{ width: `${pct}%` }} />
					</div>
					<span className="w-10 text-right font-mono text-[11px] text-subtle tabular-nums">{Math.floor(pct)}%</span>
				</div>
			)}
		</div>
	);
};

const Section = ({ title, count, children, defaultOpen = true }: { title: string; count: number; children: React.ReactNode; defaultOpen?: boolean }) => {
	const [open, setOpen] = useState(defaultOpen);
	if (count === 0) return null;
	return (
		<div>
			<button onClick={() => setOpen(!open)} className="flex w-full items-center gap-2 bg-white/[0.015] px-4 py-2 text-left text-[11px] font-medium tracking-wide text-subtle uppercase hover:text-muted">
				<ChevronRight size={12} className={cn('transition-transform', open && 'rotate-90')} />
				{title}
				<span className="font-mono">{count}</span>
			</button>
			{open && <div className="divide-y divide-line">{children}</div>}
		</div>
	);
};

const statusLine = (plan: ClientPlan | null | undefined): [string, string] => {
	if (!plan) return ['Ready.', 'text-fg'];
	const size = `${plan.downloadFiles} file${plan.downloadFiles === 1 ? '' : 's'} · ${formatBytes(plan.downloadBytes)}`;
	switch (plan.status) {
		case 'notInstalled':
			return [`Client not installed — ${formatBytes(plan.downloadBytes)} to download`, 'text-amber-400'];
		case 'updateAvailable':
			return [`Update available — ${size}`, 'text-violet-300'];
		case 'repairNeeded':
			return [plan.downloadFiles ? `${size} need repair` : 'Cleanup needed', 'text-amber-400'];
		default:
			return ['Your OctoWoW client is up to date!', 'text-green-400'];
	}
};

export const GameUpdatesPage = () => {
	const { settings } = useSettings();
	const plan = useQuery({ queryKey: ['clientUpdate'], queryFn: api.clientStatus });
	const job = useJobs().find(j => CLIENT_JOBS.includes(j.label) && j.status === 'running');
	const progress = useClientProgress();
	const log = useClientLog();
	const consoleRef = useRef<HTMLDivElement>(null);

	const run = (fn: () => Promise<unknown>) => fn().catch(e => toast.error(errorText(e)));

	// Quick check on startup and whenever the game folder changes.
	useEffect(() => {
		if (settings.wowDir) api.clientCheck().catch(() => undefined);
	}, [settings.wowDir]);

	useEffect(() => {
		consoleRef.current?.scrollTo({ top: consoleRef.current.scrollHeight });
	}, [log]);

	const p = plan.data;
	const doneByPath = useMemo(() => new Map(progress?.files.map(f => [f.path, f.done])), [progress]);
	const toDownload = p?.files.filter(f => f.download) ?? [];
	// Client Tweaks DLLs are ignored entirely, so they're not listed.
	const untouched = p?.files.filter(f => !f.download && f.state !== 'ok' && f.state !== 'managed') ?? [];
	const [status, statusColor] = job ? [job.message || `${job.label}…`, 'text-fg'] : statusLine(p);

	const pct = progress && progress.bytesTotal ? (progress.bytesDone / progress.bytesTotal) * 100 : job?.total ? (job.done / job.total) * 100 : null;
	const eta = progress && progress.speed ? (progress.bytesTotal - progress.bytesDone) / progress.speed : NaN;

	return (
		<>
			<div className="mx-auto flex max-w-[600px] flex-col items-center pt-4 text-center">
				<div className="brand-gradient mb-4 flex h-14 w-14 items-center justify-center rounded-2xl text-white shadow-[inset_0_1px_0_rgb(255_255_255/0.25),0_12px_32px_-8px_rgb(139_92_246/0.8)]">
					<CloudDownload size={26} />
				</div>
				<h1 className="text-[22px] font-semibold tracking-tight">OctoWoW Client Updater</h1>
				<p className="mt-1 text-[13px] text-muted">Synchronizes your core game files with the official OctoWoW release. Every piece is verified against the release checksums.</p>

				<div className={cn('mt-7 text-sm font-semibold', statusColor)}>{status}</div>
				{p && !job && (
					<div className="mt-1 text-xs text-subtle">
						Release <span className="font-mono">{p.release.infoHash.slice(0, 8)}</span>
						{p.release.published && ` · published ${new Date(p.release.published).toLocaleDateString()}`} · {p.release.files} files · {formatBytes(p.release.totalSize)}
						{p.baseline ? ` · last synced ${ago(p.baseline.syncedAt)}` : ''}
						{p.status === 'upToDate' && (p.deepVerified ? ' · all files verified' : p.sizeOnly ? ' · checked by size only' : '')}
					</div>
				)}

				<div className="mt-4 h-1.5 w-full overflow-hidden rounded-full bg-white/[0.06]">
					<div
						className={cn('brand-gradient h-full rounded-full transition-[width] duration-300', job && pct === null && 'w-1/3 animate-pulse')}
						style={pct !== null ? { width: `${pct}%` } : !job ? { width: p?.status === 'upToDate' ? '100%' : '0%' } : undefined}
					/>
				</div>
				<div className="mt-2 flex h-4 w-full justify-between font-mono text-[11px] text-subtle tabular-nums">
					{progress ? (
						<>
							<span>
								{formatBytes(progress.bytesDone)} / {formatBytes(progress.bytesTotal)}
							</span>
							<span>{progress.phase === 'connecting' ? 'Connecting…' : `${formatBytes(progress.speed)}/s · ETA ${duration(eta)}`}</span>
							<span>
								{progress.connections} {progress.connections === 1 ? 'connection' : 'connections'} · OctoWoW mirror
							</span>
						</>
					) : pct !== null ? (
						<span className="mx-auto">{Math.floor(pct)}%</span>
					) : null}
				</div>

				<div className="mt-5 flex gap-2">
					{job ? (
						<Button icon={Square} onClick={() => run(api.clientCancel)} className="h-10 px-5">
							Cancel
						</Button>
					) : (
						<>
							{p && (p.downloadFiles > 0 || p.obsolete.length > 0) ? (
								<Button variant="primary" icon={CloudDownload} onClick={() => run(api.clientUpdate)} className="h-10 px-5">
									{p.status === 'notInstalled' ? 'Install Client' : p.downloadFiles ? `Update ${p.downloadFiles} file${p.downloadFiles === 1 ? '' : 's'}` : 'Clean Up'}
								</Button>
							) : (
								<Button variant="primary" icon={RefreshCw} onClick={() => run(api.clientCheck)} className="h-10 px-5" disabled={!settings.wowDir}>
									Check for Game Updates
								</Button>
							)}
							{p && (p.downloadFiles > 0 || p.obsolete.length > 0) && <Button icon={RefreshCw} onClick={() => run(api.clientCheck)} className="h-10" title="Check again" />}
							<Button icon={ShieldCheck} onClick={() => run(api.clientVerify)} className="h-10" disabled={!settings.wowDir} title="Hash-check every file against the release">
								Verify Files
							</Button>
						</>
					)}
				</div>
			</div>

			{p && (
				<Panel
					title={toDownload.length || p.obsolete.length ? 'What will be updated' : 'Client files'}
					description={
						toDownload.length
							? `${toDownload.length} file${toDownload.length === 1 ? '' : 's'} to download, ${formatBytes(p.downloadBytes)} in total`
							: p.sizeOnly
								? 'Everything matches by size. Run Verify Files to compare contents.'
								: 'Nothing to download.'
					}
					className="mt-8 text-left"
				>
					<Section title="Will be downloaded" count={toDownload.length}>
						{toDownload.map(f => (
							<FileRow key={f.path} f={f} done={doneByPath.get(f.path)} />
						))}
					</Section>
					<Section title="Obsolete — will be removed" count={p.obsolete.length}>
						{p.obsolete.map(o => (
							<div key={o.path} className="flex items-center justify-between px-4 py-2.5 text-[13px]">
								<span>{o.path}</span>
								<span className="font-mono text-[11px] text-muted">{formatBytes(o.size)}</span>
							</div>
						))}
					</Section>
					<Section title="Left untouched" count={untouched.length} defaultOpen={false}>
						{untouched.map(f => (
							<FileRow key={f.path} f={f} />
						))}
					</Section>
					<Section title="No longer in the release (kept on disk)" count={p.dropped.length} defaultOpen={false}>
						{p.dropped.map(d => (
							<div key={d} className="px-4 py-2.5 text-[13px] text-muted">
								{d}
							</div>
						))}
					</Section>
				</Panel>
			)}

			<Panel icon={Terminal} title="Activity" className={cn('text-left', !p && 'mt-8')}>
				<div ref={consoleRef} className="h-44 overflow-y-auto bg-black/30 px-4 py-3 font-mono text-[11px] leading-relaxed select-text">
					{log.length === 0 ? (
						<span className="text-subtle">No activity yet.</span>
					) : (
						log.map(l => (
							<div key={l.id} className={{ info: 'text-muted', ok: 'text-green-400', warn: 'text-amber-300', error: 'text-rose-400' }[l.level]}>
								<span className="text-subtle/60">{l.time} </span>
								{l.text}
							</div>
						))
					)}
				</div>
			</Panel>
		</>
	);
};
