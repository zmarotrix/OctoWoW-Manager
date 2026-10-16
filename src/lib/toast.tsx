import { useSyncExternalStore } from 'react';
import { CircleAlert, CircleCheck, Info } from 'lucide-react';

type Kind = 'info' | 'success' | 'error';
type Toast = { id: number; kind: Kind; text: string };

let toasts: Toast[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach(l => l());

const push = (kind: Kind, text: string) => {
	const id = nextId++;
	toasts = [...toasts, { id, kind, text }];
	notify();
	window.setTimeout(() => {
		toasts = toasts.filter(t => t.id !== id);
		notify();
	}, kind === 'error' ? 6000 : 3500);
};

export const toast = {
	info: (text: string) => push('info', text),
	success: (text: string) => push('success', text),
	error: (text: string) => push('error', text)
};

const subscribe = (l: () => void) => {
	listeners.add(l);
	return () => listeners.delete(l);
};

const icons = {
	info: <Info size={16} className="text-info" />,
	success: <CircleCheck size={16} className="text-success" />,
	error: <CircleAlert size={16} className="text-danger" />
};

export const Toasts = () => {
	const list = useSyncExternalStore(subscribe, () => toasts);
	return (
		<div className="pointer-events-none fixed top-12 right-4 z-50 flex w-80 flex-col gap-2">
			{list.map(t => (
				<div
					key={t.id}
					className="animate-enter flex items-start gap-2.5 rounded-xl border border-line-strong bg-raised/90 px-3.5 py-3 text-[13px] shadow-xl shadow-black/50 backdrop-blur-xl"
				>
					<span className="mt-px shrink-0">{icons[t.kind]}</span>
					<span className="leading-snug">{t.text}</span>
				</div>
			))}
		</div>
	);
};
