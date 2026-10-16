import { getCurrentWindow } from '@tauri-apps/api/window';
import { Minus, Square, X } from 'lucide-react';
import { cn } from './ui';

const ControlButton = ({ onClick, children, close }: { onClick: () => void; children: React.ReactNode; close?: boolean }) => (
	<button
		onClick={onClick}
		className={cn(
			'flex h-10 w-11 items-center justify-center text-subtle transition-colors duration-75',
			close ? 'hover:bg-danger hover:text-white' : 'hover:bg-white/[0.06] hover:text-fg'
		)}
	>
		{children}
	</button>
);

/** Frameless-window title bar: drag region plus window controls. */
export const TitleBar = ({ title }: { title: string }) => {
	const win = getCurrentWindow();
	return (
		<div data-tauri-drag-region className="flex h-10 shrink-0 items-center justify-between pl-8">
			<span data-tauri-drag-region className="text-xs text-subtle">
				{title}
			</span>
			<div className="flex">
				<ControlButton onClick={() => win.minimize()}>
					<Minus size={15} />
				</ControlButton>
				<ControlButton onClick={() => win.toggleMaximize()}>
					<Square size={12} />
				</ControlButton>
				<ControlButton onClick={() => win.close()} close>
					<X size={16} />
				</ControlButton>
			</div>
		</div>
	);
};
