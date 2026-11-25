import { forwardRef, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode, type TextareaHTMLAttributes } from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { Dialog as D, Switch as S } from "radix-ui";
import { Loader2, Search, X, type LucideIcon } from "lucide-react";
import { cn, initials } from "@/lib/utils";

/* Buttons follow DESIGN.md (Claude, dark): coral primary used once per workspace,
   button-secondary-on-dark (elevated fill), an outline on the canvas, and a quiet
   text-only variant for icon controls. */
const button = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-control font-medium select-none transition-colors duration-150 ease-standard disabled:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary: "border border-transparent bg-primary text-on-primary active:bg-primary-active disabled:border-hairline disabled:bg-elevated disabled:text-disabled",
        secondary: "border border-hairline bg-elevated text-ink hover:bg-hover disabled:text-disabled",
        outline: "border border-hairline-strong bg-transparent text-ink hover:bg-elevated disabled:text-disabled",
        ghost: "text-muted hover:bg-elevated hover:text-ink disabled:text-disabled",
        danger: "border border-danger/50 bg-transparent text-danger hover:border-danger disabled:opacity-40",
      },
      size: {
        sm: "h-8 px-3 text-[13px]",
        md: "h-10 px-5 text-sm",
        icon: "size-9 rounded-full",
        iconSm: "size-7 [&_svg]:size-3.5",
      },
    },
    defaultVariants: { variant: "ghost", size: "md" },
  },
);

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & VariantProps<typeof button> & { icon?: LucideIcon; busy?: boolean };

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { className, variant, size, icon: Icon, busy, children, disabled, ...props },
  ref,
) {
  return (
    <button ref={ref} className={cn(button({ variant, size }), className)} disabled={disabled || busy} aria-busy={busy || undefined} {...props}>
      {busy ? <Loader2 className="animate-spin" /> : Icon ? <Icon /> : null}
      {children}
    </button>
  );
});

export function IconButton({ icon, label, ...props }: Omit<ButtonProps, "children"> & { label: string; icon: LucideIcon }) {
  return <Button size="icon" icon={icon} title={label} aria-label={label} {...props} />;
}

/* text-input / text-input-focused: recessed fill, findable boundary, and on focus a coral
   border with a 3px coral ring at 15%. */
const field =
  "w-full rounded-control border border-control bg-deep px-3.5 text-sm text-ink placeholder:text-muted-soft transition-[border-color,box-shadow] duration-150 focus:border-primary focus:shadow-[0_0_0_3px_rgb(204_120_92/0.15)] focus-visible:outline-none disabled:opacity-50";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function Input({ className, ...props }, ref) {
  return <input ref={ref} className={cn(field, "h-10", className)} spellCheck={false} {...props} />;
});

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(function Textarea(
  { className, ...props },
  ref,
) {
  return <textarea ref={ref} className={cn(field, "py-2 resize-none", className)} {...props} />;
});

export function SearchInput({ value, onChange, placeholder, className }: { value: string; onChange: (v: string) => void; placeholder: string; className?: string }) {
  return (
    <div className={cn("relative", className)}>
      <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-soft" />
      <Input value={value} onChange={(e) => onChange(e.target.value)} aria-label={placeholder} placeholder={placeholder} className="pl-9 pr-8" />
      {value && (
        <button type="button" className="absolute right-1.5 top-1/2 grid size-7 -translate-y-1/2 place-items-center rounded-full text-muted-soft hover:bg-elevated hover:text-ink" onClick={() => onChange("")} aria-label={`Clear ${placeholder.toLowerCase()}`}>
          <X className="size-3.5" />
        </button>
      )}
    </div>
  );
}

export function Select({ value, onChange, children, className }: { value: string; onChange: (v: string) => void; children: ReactNode; className?: string }) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)} className={cn(field, "h-10 appearance-none pr-8 bg-[url('data:image/svg+xml;utf8,<svg xmlns=%22http://www.w3.org/2000/svg%22 width=%2216%22 height=%2216%22 viewBox=%220 0 24 24%22 fill=%22none%22 stroke=%22%238c8c8e%22 stroke-width=%222%22><path d=%22m6 9 6 6 6-6%22/></svg>')] bg-no-repeat bg-[right_10px_center]", className)}>
      {children}
    </select>
  );
}

export function Switch({ checked, onChange, disabled, label, description }: { checked: boolean; onChange: (v: boolean) => void; disabled?: boolean; label: string; description?: string }) {
  return (
    <label className={cn("flex items-center justify-between gap-4 py-3", disabled && "opacity-40")}>
      <span className="min-w-0">
        <span className="block text-sm text-ink">{label}</span>
        {description && <span className="mt-0.5 block text-[13px] text-muted-soft">{description}</span>}
      </span>
      <S.Root
        checked={checked}
        onCheckedChange={onChange}
        disabled={disabled}
        className="peer relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-control bg-elevated transition-colors duration-150 data-[state=checked]:border-primary data-[state=checked]:bg-primary"
      >
        <S.Thumb className="block size-3.5 translate-x-[2px] rounded-full bg-muted transition-transform duration-150 ease-decel data-[state=checked]:translate-x-[17px] data-[state=checked]:bg-on-primary" />
      </S.Root>
    </label>
  );
}

/** badge-pill, optionally led by a status-dot. */
export function Badge({ children, dot, className, icon: Icon, title }: { children: ReactNode; dot?: string; className?: string; icon?: LucideIcon; title?: string }) {
  return (
    <span title={title} className={cn("inline-flex min-h-6 max-w-full items-center gap-1.5 rounded-full border border-hairline bg-elevated px-2.5 py-0.5 text-xs text-body", className)}>
      {dot && <StatusDot color={dot} />}
      {Icon && <Icon className="size-3.5 shrink-0" />}
      <span className="truncate">{children}</span>
    </span>
  );
}

export function StatusDot({ color, pulse, className }: { color: string; pulse?: boolean; className?: string }) {
  return <span aria-hidden className={cn("size-2 shrink-0 rounded-full", pulse && "animate-pulse", className)} style={{ background: color }} />;
}

/** feature-card, or the dark code-window surface for a workspace's focal pane. */
export function Card({ className, children, tone, ...props }: React.HTMLAttributes<HTMLDivElement> & { tone?: "window" }) {
  return (
    <div className={cn(tone === "window" ? "surface-window" : "surface-card", className)} {...props}>
      {children}
    </div>
  );
}

/** hero-band: the workspace's serif headline, a sub-line, and its one coral action. */
export function WorkspaceHeading({ title, detail, eyebrow, children }: { title: ReactNode; detail?: ReactNode; eyebrow?: ReactNode; children?: ReactNode }) {
  return (
    <header className="workspace-heading flex shrink-0 flex-wrap items-end justify-between gap-x-6 gap-y-4 pt-2 pb-6 sm:pt-4 sm:pb-8">
      <div className="min-w-0">
        {eyebrow && <div className="mb-4 flex flex-wrap gap-2">{eyebrow}</div>}
        <h1 className="display-hero text-balance">{title}</h1>
        {detail != null && <p className="subtitle mt-3 text-muted tabular-nums">{detail}</p>}
      </div>
      {children && <div className="flex min-w-0 flex-wrap items-center gap-2">{children}</div>}
    </header>
  );
}

export function Spinner({ className }: { className?: string }) {
  return <Loader2 className={cn("size-4 animate-spin text-muted-soft", className)} />;
}

export function Center({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("flex h-full w-full items-center justify-center", className)}>{children}</div>;
}

/** Type-led: no decorative tile, just a small mark, a heading and one instruction. */
export function EmptyState({ icon: Icon, title, hint, children }: { icon: LucideIcon; title: string; hint?: string; children?: ReactNode }) {
  return (
    <div className="flex h-full min-h-0 flex-1 overflow-y-auto p-6 animate-fade">
      <div className="m-auto flex max-w-sm flex-col items-center gap-2 text-center">
        <Icon className="mb-2 size-5 text-muted-soft" aria-hidden />
        <h2 className="heading-sm">{title}</h2>
        {hint && <div className="text-sm text-muted-soft">{hint}</div>}
        {children}
      </div>
    </div>
  );
}

export function Avatar({ name, size = 36 }: { name: string; size?: number }) {
  return (
    <span
      className="flex shrink-0 items-center justify-center rounded-full border border-hairline-strong bg-card font-medium text-ink"
      style={{ width: size, height: size, fontSize: size * 0.36 }}
    >
      {initials(name)}
    </span>
  );
}

/** The tab strip at the top of a pane: title, detail and the pane's controls. */
export function PaneHeader({ title, subtitle, leading, children }: { title: string; subtitle?: ReactNode; leading?: ReactNode; children?: ReactNode }) {
  return (
    <header className="flex min-h-16 shrink-0 flex-wrap items-center gap-2 border-b border-hairline px-4 py-2.5 sm:px-6">
      {leading}
      <div className="min-w-0 flex-1">
        <h2 className="truncate text-lg font-medium tracking-[-0.3px] text-ink" title={title}>{title}</h2>
        {subtitle && <div className="truncate text-[13px] text-muted-soft">{subtitle}</div>}
      </div>
      {children}
    </header>
  );
}

/** category-tab: an in-page filter. */
export function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        "nav-label inline-flex h-8 shrink-0 items-center gap-1.5 rounded-control px-3.5 text-[13px] transition-colors duration-150",
        active ? "bg-elevated text-ink" : "text-muted hover:text-ink",
      )}
    >
      {children}
    </button>
  );
}

/** code-window-card: editor chrome dots, an optional tab, and a JetBrains Mono body on
 *  the softer inner block. */
export function CodeWindow({ tab, children, className }: { tab?: string; children: ReactNode; className?: string }) {
  return (
    <div className={cn("surface-window overflow-hidden", className)}>
      <div className="flex h-10 items-center gap-3 border-b border-hairline px-4">
        <span aria-hidden className="flex gap-1.5">
          <span className="size-2.5 rounded-full bg-danger" />
          <span className="size-2.5 rounded-full bg-warning" />
          <span className="size-2.5 rounded-full bg-success" />
        </span>
        {tab && <span className="rounded-sm bg-elevated px-2.5 py-1 font-mono text-xs text-muted">{tab}</span>}
      </div>
      <code className="selectable block overflow-x-auto bg-card px-5 py-4 font-mono text-[13px] leading-[1.6] whitespace-pre text-body">{children}</code>
    </div>
  );
}

/** Shared surfaces for Radix menus. */
export const menuSurface = "z-50 min-w-44 rounded-card border border-hairline-strong bg-elevated p-1.5 animate-pop";
export const menuItem = "flex h-8 cursor-default items-center gap-2 rounded-sm px-2.5 text-[13px] text-ink outline-none data-[highlighted]:bg-hover [&_svg]:size-4 [&_svg]:text-muted";
export const menuSeparator = "my-1 h-px bg-hairline";

export function Dialog({ open, onOpenChange, title, description, children, className }: { open: boolean; onOpenChange: (v: boolean) => void; title: string; description?: string; children: ReactNode; className?: string }) {
  return (
    <D.Root open={open} onOpenChange={onOpenChange}>
      <D.Portal>
        <D.Overlay className="fixed inset-0 z-40 bg-scrim animate-fade" />
        <D.Content
          className={cn("fixed left-1/2 top-1/2 z-50 max-h-[calc(100dvh-24px)] w-[min(480px,calc(100vw-24px))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-card border border-hairline-strong bg-elevated p-6 outline-none animate-pop sm:p-8", className)}
        >
          <D.Title className="heading-sm">{title}</D.Title>
          {description ? <D.Description className="mt-2 text-sm text-muted">{description}</D.Description> : <D.Description className="sr-only">{title}</D.Description>}
          <div className="mt-6">{children}</div>
        </D.Content>
      </D.Portal>
    </D.Root>
  );
}
