import { useEffect, useSyncExternalStore, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { cn } from '../components/ui';

/** Modal shell: dimmed backdrop, Esc / backdrop click to close. */
export const Modal = ({ title, subtitle, onClose, children, footer, width = 'max-w-md' }: { title: string; subtitle?: string; onClose: () => void; children?: ReactNode; footer: ReactNode; width?: string }) => {
	useEffect(() => {
		const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
		window.addEventListener('keydown', onKey);
		return () => window.removeEventListener('keydown', onKey);
	}, [onClose]);

	return createPortal(
		<div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6 backdrop-blur-sm" onMouseDown={e => e.target === e.currentTarget && onClose()}>
			<div role="dialog" aria-modal className={cn('animate-enter w-full rounded-2xl border border-line-strong bg-raised shadow-2xl shadow-black/60', width)}>
				<div className="px-6 pt-5">
					<h2 className="text-base font-semibold tracking-tight">{title}</h2>
					{subtitle && <p className="mt-1 text-[13px] text-muted">{subtitle}</p>}
				</div>
				{children && <div className="px-6 pt-4">{children}</div>}
				<div className="mt-5 flex justify-end gap-2 border-t border-line px-6 py-4">{footer}</div>
			</div>
		</div>,
		document.body
	);
};

// ---------- promise-based confirm ----------

type ConfirmOptions = { title: string; message?: string; confirmLabel?: string; danger?: boolean };
type Pending = ConfirmOptions & { resolve: (ok: boolean) => void };

let pending: Pending | null = null;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach(l => l());

export const confirm = (opts: ConfirmOptions) =>
	new Promise<boolean>(resolve => {
		pending?.resolve(false);
		pending = { ...opts, resolve };
		notify();
	});

const close = (ok: boolean) => {
	pending?.resolve(ok);
	pending = null;
	notify();
};

const subscribe = (l: () => void) => {
	listeners.add(l);
	return () => listeners.delete(l);
};

export const ConfirmHost = () => {
	const p = useSyncExternalStore(subscribe, () => pending);
	if (!p) return null;
	return (
		<Modal
			title={p.title}
			subtitle={p.message}
			onClose={() => close(false)}
			footer={
				<>
					<button onClick={() => close(false)} className="h-8 rounded-lg border border-line-strong bg-white/[0.04] px-3 text-[13px] font-medium hover:bg-white/[0.08]">
						Cancel
					</button>
					<button
						autoFocus
						onClick={() => close(true)}
						className={cn('h-8 rounded-lg px-3 text-[13px] font-medium text-white', p.danger ? 'bg-danger hover:brightness-110' : 'brand-gradient hover:brightness-110')}
					>
						{p.confirmLabel ?? 'OK'}
					</button>
				</>
			}
		/>
	);
};
