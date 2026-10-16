import { Check, Loader2, X } from 'lucide-react';
import { useJobs } from '../lib/jobs';
import { cn } from './ui';

/** Floating tray of background jobs, bottom-center of the content area. */
export const JobBar = () => {
	const jobs = useJobs();
	if (jobs.length === 0) return null;

	return (
		<div className="pointer-events-none absolute inset-x-0 bottom-5 z-30 flex justify-center px-8">
			<div className="pointer-events-auto flex w-full max-w-lg flex-col gap-3 rounded-2xl border border-line-strong bg-raised/90 p-3.5 shadow-2xl shadow-black/60 backdrop-blur-xl">
				{jobs.map(j => {
					const pct = j.status === 'running' ? (j.total > 0 ? (j.done / j.total) * 100 : null) : 100;
					return (
						<div key={j.id} className="animate-enter flex items-center gap-3">
							<div
								className={cn(
									'flex h-7 w-7 shrink-0 items-center justify-center rounded-lg',
									j.status === 'running' && 'bg-accent-soft text-accent',
									j.status === 'finished' && 'bg-success/15 text-success',
									j.status === 'failed' && 'bg-danger/15 text-danger'
								)}
							>
								{j.status === 'running' ? <Loader2 size={14} className="animate-spin" /> : j.status === 'finished' ? <Check size={14} /> : <X size={14} />}
							</div>
							<div className="min-w-0 flex-1">
								<div className="flex items-baseline justify-between gap-3">
									<span className="truncate text-[13px] font-medium">{j.label}</span>
									{j.status === 'running' && j.total > 0 && (
										<span className="font-mono text-[11px] text-subtle tabular-nums">
											{/* Small totals are item counts; large ones are bytes. */}
											{j.total <= 100_000 ? `${j.done}/${j.total}` : `${Math.floor((j.done / j.total) * 100)}%`}
										</span>
									)}
								</div>
								<div className={cn('truncate text-xs', j.status === 'failed' ? 'text-rose-400' : 'text-subtle')}>{j.message || 'Starting…'}</div>
								<div className="mt-1.5 h-1 overflow-hidden rounded-full bg-white/[0.06]">
									<div
										className={cn(
											'h-full rounded-full transition-[width] duration-200',
											j.status === 'failed' ? 'bg-danger' : j.status === 'finished' ? 'bg-success' : 'brand-gradient',
											pct === null && 'w-1/3 animate-pulse'
										)}
										style={pct !== null ? { width: `${pct}%` } : undefined}
									/>
								</div>
							</div>
						</div>
					);
				})}
			</div>
		</div>
	);
};
