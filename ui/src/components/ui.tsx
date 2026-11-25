import { forwardRef, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode, type TextareaHTMLAttributes } from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { Dialog as D, Switch as S } from "radix-ui";
import { Loader2, Search, X, type LucideIcon } from "lucide-react";
import { cn, initials } from "@/lib/utils";

const button = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-control font-medium select-none transition-colors duration-150 ease-standard disabled:pointer-events-none disabled:opacity-40 [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary: "border border-transparent bg-primary text-primary-fg hover:bg-primary-hover active:bg-pressed disabled:bg-disabled disabled:text-disabled-fg disabled:opacity-100",
        ghost: "text-muted hover:bg-hover hover:text-fg",
        outline: "border border-control-border bg-raised text-fg hover:bg-hover",
        danger: "border border-danger/35 bg-danger-surface text-danger hover:border-danger",
      },
      size: {
        sm: "h-9 px-3 text-[13px]",
        md: "h-10 px-4 text-sm",
        icon: "size-9 rounded-full",
        iconSm: "size-7 rounded-full [&_svg]:size-3.5",
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

const field =
  "w-full rounded-control border border-control-border bg-deep px-3.5 text-sm text-fg placeholder:text-subtle transition-colors duration-150 focus:border-accent disabled:opacity-50";

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
      <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-subtle" />
      <Input value={value} onChange={(e) => onChange(e.target.value)} aria-label={placeholder} placeholder={placeholder} className="pl-9 pr-8" />
      {value && (
        <button type="button" className="absolute right-1.5 top-1/2 grid size-7 -translate-y-1/2 place-items-center rounded-full text-subtle hover:bg-hover hover:text-fg" onClick={() => onChange("")} aria-label={`Clear ${placeholder.toLowerCase()}`}>
          <X className="size-3.5" />
        </button>
      )}
    </div>
  );
}

export function Select({ value, onChange, children, className }: { value: string; onChange: (v: string) => void; children: ReactNode; className?: string }) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)} className={cn(field, "h-10 appearance-none pr-8 bg-[url('data:image/svg+xml;utf8,<svg xmlns=%22http://www.w3.org/2000/svg%22 width=%2216%22 height=%2216%22 viewBox=%220 0 24 24%22 fill=%22none%22 stroke=%22%23888e90%22 stroke-width=%222%22><path d=%22m6 9 6 6 6-6%22/></svg>')] bg-no-repeat bg-[right_10px_center]", className)}>
      {children}
    </select>
  );
}

export function Switch({ checked, onChange, disabled, label, description }: { checked: boolean; onChange: (v: boolean) => void; disabled?: boolean; label: string; description?: string }) {
  return (
    <label className={cn("flex items-center justify-between gap-4 rounded-control px-1 py-2", disabled && "opacity-40")}>
      <span className="min-w-0">
        <span className="block text-[13px] text-fg">{label}</span>
        {description && <span className="block text-xs text-subtle">{description}</span>}
      </span>
      <S.Root
        checked={checked}
        onCheckedChange={onChange}
        disabled={disabled}
        className="peer relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-control-border bg-raised transition-colors duration-150 data-[state=checked]:border-accent data-[state=checked]:bg-selected"
      >
        <S.Thumb className="block size-3.5 translate-x-[3px] rounded-full bg-muted transition-transform duration-150 ease-decel data-[state=checked]:translate-x-[17px] data-[state=checked]:bg-accent" />
      </S.Root>
    </label>
  );
}

export function Badge({ children, dot, className, icon: Icon, title }: { children: ReactNode; dot?: string; className?: string; icon?: LucideIcon; title?: string }) {
  return (
    <span title={title} className={cn("inline-flex min-h-7 max-w-full items-center gap-1.5 rounded-full border border-border bg-raised px-2.5 text-[13px] text-body", className)}>
      {dot && <span className="size-1.5 shrink-0 rounded-full" style={{ background: dot }} />}
      {Icon && <Icon className="size-3.5 shrink-0" />}
      <span className="truncate">{children}</span>
    </span>
  );
}

export function Card({ className, children, tone, ...props }: React.HTMLAttributes<HTMLDivElement> & { tone?: "workbench" }) {
  return (
    <div className={cn(tone === "workbench" ? "workbench" : "rounded-card border border-border bg-surface", className)} {...props}>
      {children}
    </div>
  );
}

export function WorkspaceHeading({ title, detail, children }: { title: string; detail?: ReactNode; children?: ReactNode }) {
  return (
    <header className="workspace-heading flex shrink-0 flex-wrap items-center justify-between gap-x-4 gap-y-2 pb-4 sm:pb-6">
      <div className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-1 sm:gap-x-4">
        <h1 className="workspace-title">{title}</h1>
        {detail != null && <span className="text-[13px] text-subtle tabular-nums">{detail}</span>}
      </div>
      {children && <div className="flex min-w-0 flex-wrap items-center gap-2">{children}</div>}
    </header>
  );
}

export function Spinner({ className }: { className?: string }) {
  return <Loader2 className={cn("size-4 animate-spin text-subtle", className)} />;
}

export function Center({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("flex h-full w-full items-center justify-center", className)}>{children}</div>;
}

export function EmptyState({ icon: Icon, title, hint, children }: { icon: LucideIcon; title: string; hint?: string; children?: ReactNode }) {
  return (
    <div className="flex h-full min-h-0 flex-1 overflow-y-auto p-4 animate-fade sm:p-6">
      <div className="m-auto flex max-w-xs flex-col items-center gap-2 text-center">
        <div className="mb-2 flex size-12 items-center justify-center rounded-card border border-border-strong bg-raised">
          <Icon className="size-5 text-accent" aria-hidden />
        </div>
        <h2 className="page-title">{title}</h2>
        {hint && <div className="text-sm text-subtle">{hint}</div>}
        {children}
      </div>
    </div>
  );
}

export function Avatar({ name, size = 36 }: { name: string; size?: number }) {
  return (
    <span
      className="flex shrink-0 items-center justify-center rounded-full border border-border-strong bg-raised font-medium text-fg"
      style={{ width: size, height: size, fontSize: size * 0.36 }}
    >
      {initials(name)}
    </span>
  );
}

export function PaneHeader({ title, subtitle, leading, children }: { title: string; subtitle?: ReactNode; leading?: ReactNode; children?: ReactNode }) {
  return (
    <header className="flex min-h-16 shrink-0 flex-wrap items-center gap-2 border-b border-border bg-surface px-3 py-2 sm:px-5">
      {leading}
      <div className="min-w-0 flex-1">
        <h2 className="page-title truncate" title={title}>{title}</h2>
        {subtitle && <div className="truncate text-xs text-subtle">{subtitle}</div>}
      </div>
      {children}
    </header>
  );
}

/** A category tab for an in-page filter. */
export function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        "inline-flex h-9 shrink-0 items-center gap-1.5 rounded-control border px-3 text-sm transition-colors duration-150",
        active ? "border-border-strong bg-selected font-medium text-fg" : "border-transparent text-muted hover:bg-hover hover:text-fg",
      )}
    >
      {children}
    </button>
  );
}

export function Dialog({ open, onOpenChange, title, description, children, className }: { open: boolean; onOpenChange: (v: boolean) => void; title: string; description?: string; children: ReactNode; className?: string }) {
  return (
    <D.Root open={open} onOpenChange={onOpenChange}>
      <D.Portal>
        <D.Overlay className="fixed inset-0 z-40 bg-scrim animate-fade" />
        <D.Content
          className={cn("fixed left-1/2 top-1/2 z-50 max-h-[calc(100dvh-24px)] w-[min(460px,calc(100vw-24px))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-card border border-border-strong bg-raised p-6 outline-none animate-pop", className)}
        >
          <D.Title className="page-title">{title}</D.Title>
          {description ? <D.Description className="mt-2 text-sm text-muted">{description}</D.Description> : <D.Description className="sr-only">{title}</D.Description>}
          <div className="mt-4">{children}</div>
        </D.Content>
      </D.Portal>
    </D.Root>
  );
}
