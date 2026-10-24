import type { ButtonHTMLAttributes, CSSProperties, InputHTMLAttributes, ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import { ChevronDown } from 'lucide-react';

export const cn = (...classes: (string | false | null | undefined)[]) => classes.filter(Boolean).join(' ');

/* ---------- Layout ---------- */

export const PageHeader = ({ title, description, actions }: { title: string; description?: string; actions?: ReactNode }) => (
	<header className="mb-6 flex items-end justify-between gap-6">
		<div>
			<h1 className="text-[22px] font-semibold tracking-tight">{title}</h1>
			{description && <p className="mt-1 max-w-xl text-[13px] text-muted">{description}</p>}
		</div>
		{actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
	</header>
);

export const Panel = ({
	icon: Icon,
	title,
	description,
	actions,
	children,
	className
}: {
	icon?: LucideIcon;
	title?: string;
	description?: string;
	actions?: ReactNode;
	children: ReactNode;
	className?: string;
}) => (
	<section className={cn('mb-4 overflow-hidden rounded-xl border border-line bg-panel', className)}>
		{title && (
			<div className="flex items-center justify-between gap-4 border-b border-line px-5 py-3.5">
				<div className="flex items-center gap-3">
					{Icon && (
						<div className="flex h-7 w-7 items-center justify-center rounded-lg bg-accent-soft text-accent">
							<Icon size={15} strokeWidth={2} />
						</div>
					)}
					<div>
						<h2 className="text-[13px] font-semibold">{title}</h2>
						{description && <p className="text-xs text-subtle">{description}</p>}
					</div>
				</div>
				{actions}
			</div>
		)}
		<div className="divide-y divide-line">{children}</div>
	</section>
);

/** A label/description on the left and a control on the right. */
export const Row = ({ label, description, children, className }: { label: ReactNode; description?: ReactNode; children?: ReactNode; className?: string }) => (
	<div className={cn('flex min-h-14 items-center justify-between gap-6 px-5 py-3', className)}>
		<div className="min-w-0">
			<div className="text-[13px] font-medium">{label}</div>
			{description && <div className="mt-0.5 text-xs leading-relaxed text-subtle">{description}</div>}
		</div>
		{children && <div className="flex shrink-0 items-center gap-3">{children}</div>}
	</div>
);

export const Empty = ({ icon: Icon, title, children }: { icon: LucideIcon; title: string; children?: ReactNode }) => (
	<div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-line-strong px-6 py-14 text-center">
		<div className="mb-3 flex h-10 w-10 items-center justify-center rounded-xl border border-line bg-panel text-muted">
			<Icon size={18} />
		</div>
		<div className="text-[13px] font-medium">{title}</div>
		{children && <div className="mt-1 max-w-sm text-xs text-subtle">{children}</div>}
	</div>
);

/* ---------- Controls ---------- */

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';
const variants: Record<Variant, string> = {
	primary:
		'brand-gradient text-white shadow-[inset_0_1px_0_rgb(255_255_255/0.2),0_1px_2px_rgb(0_0_0/0.4),0_4px_14px_-4px_rgb(139_92_246/0.6)] hover:brightness-110',
	secondary: 'border border-line-strong bg-white/[0.04] text-fg hover:bg-white/[0.08]',
	ghost: 'text-muted hover:bg-white/[0.06] hover:text-fg',
	danger: 'text-muted hover:bg-danger/10 hover:text-danger'
};

export const Button = ({
	variant = 'secondary',
	icon: Icon,
	size = 'md',
	className,
	children,
	...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; icon?: LucideIcon; size?: 'sm' | 'md' }) => (
	<button
		{...props}
		className={cn(
			'inline-flex items-center justify-center gap-1.5 rounded-lg font-medium whitespace-nowrap transition-[background,filter,transform,color] duration-100 active:scale-[0.97] disabled:pointer-events-none disabled:opacity-40',
			// Icon-only buttons are square with no side padding; never emit both
			// px-* classes, since CSS order (not class order) would decide the winner.
			size === 'sm' ? 'h-7 text-xs' : 'h-8 text-[13px]',
			children ? (size === 'sm' ? 'px-2.5' : 'px-3') : size === 'sm' ? 'w-7' : 'w-8',
			variants[variant],
			className
		)}
	>
		{Icon && <Icon size={size === 'sm' ? 13 : 15} strokeWidth={2} className="shrink-0" />}
		{children}
	</button>
);

export const Switch = ({
	checked,
	onChange,
	tone = 'accent',
	label
}: {
	checked: boolean;
	onChange: (v: boolean) => void;
	tone?: 'accent' | 'danger';
	label?: string;
}) => (
	<button
		type="button"
		role="switch"
		aria-checked={checked}
		aria-label={label}
		onClick={() => onChange(!checked)}
		className={cn(
			'relative h-5 w-9 shrink-0 rounded-full border transition-colors duration-150',
			checked
				? tone === 'danger'
					? 'border-transparent bg-danger'
					: 'brand-gradient border-transparent'
				: 'border-line-strong bg-white/[0.06]'
		)}
	>
		<span
			className={cn(
				'absolute top-[2px] left-[2px] h-3.5 w-3.5 rounded-full bg-white shadow-sm transition-transform duration-150 ease-out',
				checked && 'translate-x-4'
			)}
		/>
	</button>
);

export const Segmented = <T extends string>({
	value,
	options,
	onChange,
	size = 'md'
}: {
	value: T;
	options: { value: T; label: string }[];
	onChange: (v: T) => void;
	size?: 'sm' | 'md';
}) => (
	<div className="inline-flex rounded-lg border border-line bg-black/30 p-0.5">
		{options.map(o => (
			<button
				key={o.value}
				onClick={() => onChange(o.value)}
				className={cn(
					'rounded-md font-medium transition-colors duration-100',
					size === 'sm' ? 'h-6 px-2 text-[11px]' : 'h-7 px-3 text-xs',
					value === o.value ? 'bg-white/[0.1] text-fg shadow-sm' : 'text-subtle hover:text-muted'
				)}
			>
				{o.label}
			</button>
		))}
	</div>
);

export const Select = <T extends string>({
	value,
	options,
	onChange
}: {
	value: T;
	options: { value: T; label: string }[];
	onChange: (v: T) => void;
}) => (
	<div className="relative">
		<select
			value={value}
			onChange={e => onChange(e.target.value as T)}
			className="h-8 appearance-none rounded-lg border border-line-strong bg-white/[0.04] pr-8 pl-3 text-[13px] outline-none hover:bg-white/[0.07] focus:border-accent"
		>
			{options.map(o => (
				<option key={o.value} value={o.value} className="bg-raised">
					{o.label}
				</option>
			))}
		</select>
		<ChevronDown size={14} className="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 text-subtle" />
	</div>
);

export const Input = ({ icon: Icon, className, ...props }: InputHTMLAttributes<HTMLInputElement> & { icon?: LucideIcon }) => (
	<div className={cn('relative', className)}>
		{Icon && <Icon size={14} className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-subtle" />}
		<input
			{...props}
			className={cn(
				'h-8 w-full rounded-lg border border-line-strong bg-black/30 text-[13px] outline-none placeholder:text-subtle focus:border-accent focus:bg-black/40',
				Icon ? 'pr-3 pl-8' : 'px-3'
			)}
		/>
	</div>
);

export const Slider = ({
	value,
	min,
	max,
	onChange,
	danger
}: {
	value: number;
	min: number;
	max: number;
	onChange: (v: number) => void;
	danger?: boolean;
}) => {
	const pct = `${((Math.min(value, max) - min) / (max - min)) * 100}%`;
	return (
		<div className="flex w-72 items-center gap-3">
			<input
				type="range"
				min={min}
				max={max}
				value={value}
				onChange={e => onChange(Number(e.target.value))}
				className={cn('slider flex-1', danger && 'danger')}
				style={{ '--pct': pct } as CSSProperties}
			/>
			<span className="w-14 rounded-md border border-line bg-black/30 py-0.5 text-center font-mono text-xs text-fg tabular-nums">{value}</span>
		</div>
	);
};

type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info';
const tones: Record<Tone, string> = {
	neutral: 'bg-white/[0.06] text-muted',
	accent: 'bg-accent-soft text-violet-300',
	success: 'bg-success/12 text-green-400',
	warning: 'bg-warning/12 text-amber-400',
	danger: 'bg-danger/12 text-rose-400',
	info: 'bg-info/12 text-sky-400'
};

export const Badge = ({ tone = 'neutral', children }: { tone?: Tone; children: ReactNode }) => (
	<span className={cn('inline-flex h-5 items-center rounded-md px-1.5 text-[11px] font-medium', tones[tone])}>{children}</span>
);

export const Dot = ({ tone }: { tone: 'success' | 'warning' | 'danger' | 'muted' }) => (
	<span
		className={cn(
			'inline-block h-1.5 w-1.5 rounded-full',
			{ success: 'bg-success shadow-[0_0_8px] shadow-success/70', warning: 'bg-warning shadow-[0_0_8px] shadow-warning/70', danger: 'bg-danger', muted: 'bg-subtle' }[tone]
		)}
	/>
);
