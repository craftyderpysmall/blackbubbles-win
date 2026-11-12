// shadcn-style primitives on the Neutral theme. Rules (hyprland.lua): one inverted white
// primary per surface, everything else ghost or outline; cards 16px, controls 10px,
// buttons/badges pills; 1px borders that step up to border-strong on focus.
import { forwardRef, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode, type TextareaHTMLAttributes } from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { Dialog as D, Switch as S } from "radix-ui";
import { Loader2, Search, X, type LucideIcon } from "lucide-react";
import { cn, initials } from "@/lib/utils";

const button = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-full font-medium select-none transition-[background-color,color,border-color,transform,opacity] duration-200 ease-standard active:scale-[0.97] disabled:pointer-events-none disabled:opacity-40 [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary: "bg-primary text-primary-fg hover:bg-[#e5e5e5]",
        ghost: "text-muted hover:bg-raised hover:text-fg",
        outline: "border border-border text-fg hover:bg-raised hover:border-border-strong",
        danger: "text-danger hover:bg-raised",
      },
      size: {
        sm: "h-7 px-3 text-xs",
        md: "h-8 px-3.5 text-[13px]",
        icon: "size-8",
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
    <button ref={ref} className={cn(button({ variant, size }), className)} disabled={disabled || busy} {...props}>
      {busy ? <Loader2 className="animate-spin" /> : Icon ? <Icon /> : null}
      {children}
    </button>
  );
});

export function IconButton({ icon, label, ...props }: Omit<ButtonProps, "children"> & { label: string; icon: LucideIcon }) {
  return <Button size="icon" icon={icon} title={label} aria-label={label} {...props} />;
}

const field =
  "w-full rounded-control border border-border bg-raised px-3 text-[13px] text-fg placeholder:text-subtle outline-none transition-colors duration-200 focus:border-border-strong disabled:opacity-50";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function Input({ className, ...props }, ref) {
  return <input ref={ref} className={cn(field, "h-9", className)} spellCheck={false} {...props} />;
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
      <Input value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} className="pl-9 pr-8" />
      {value && (
        <button className="absolute right-2 top-1/2 -translate-y-1/2 rounded-full p-1 text-subtle hover:text-fg" onClick={() => onChange("")} aria-label="Clear">
          <X className="size-3.5" />
        </button>
      )}
    </div>
  );
}

export function Select({ value, onChange, children, className }: { value: string; onChange: (v: string) => void; children: ReactNode; className?: string }) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)} className={cn(field, "h-9 appearance-none pr-8 bg-[url('data:image/svg+xml;utf8,<svg xmlns=%22http://www.w3.org/2000/svg%22 width=%2216%22 height=%2216%22 viewBox=%220 0 24 24%22 fill=%22none%22 stroke=%22%23737373%22 stroke-width=%222%22><path d=%22m6 9 6 6 6-6%22/></svg>')] bg-no-repeat bg-[right_10px_center]", className)}>
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
        className="peer relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-border-strong bg-raised transition-colors duration-200 data-[state=checked]:border-primary data-[state=checked]:bg-primary"
      >
        <S.Thumb className="block size-3.5 translate-x-[3px] rounded-full bg-muted transition-transform duration-300 ease-decel data-[state=checked]:translate-x-[17px] data-[state=checked]:bg-primary-fg" />
      </S.Root>
    </label>
  );
}

export function Badge({ children, dot, className, icon: Icon, title }: { children: ReactNode; dot?: string; className?: string; icon?: LucideIcon; title?: string }) {
  return (
    <span title={title} className={cn("inline-flex h-6 max-w-full items-center gap-1.5 rounded-full border border-border bg-raised px-2.5 text-xs text-muted", className)}>
      {dot && <span className="size-1.5 shrink-0 rounded-full" style={{ background: dot }} />}
      {Icon && <Icon className="size-3.5 shrink-0" />}
      <span className="truncate">{children}</span>
    </span>
  );
}

export function Card({ className, children, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cn("rounded-card border border-border bg-surface", className)} {...props}>
      {children}
    </div>
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
    <Center className="animate-fade p-6">
      <div className="flex max-w-xs flex-col items-center gap-2 text-center">
        <div className="mb-1 flex size-11 items-center justify-center rounded-full border border-border bg-raised">
          <Icon className="size-5 text-muted" />
        </div>
        <div className="text-sm font-semibold text-fg">{title}</div>
        {hint && <div className="text-xs text-subtle">{hint}</div>}
        {children}
      </div>
    </Center>
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

export function PaneHeader({ title, subtitle, children }: { title: ReactNode; subtitle?: ReactNode; children?: ReactNode }) {
  return (
    <div className="flex min-h-12 items-center gap-2 px-4 pt-3 pb-2">
      <div className="min-w-0 flex-1">
        <div className="truncate text-[15px] font-semibold text-fg">{title}</div>
        {subtitle && <div className="truncate text-xs text-subtle">{subtitle}</div>}
      </div>
      {children}
    </div>
  );
}

/** A filter pill. The selected one is the surface's single inverted primary. */
export function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full border px-3 text-xs transition-colors duration-200",
        active ? "border-primary bg-primary font-medium text-primary-fg" : "border-border text-muted hover:bg-raised hover:text-fg",
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
        <D.Overlay className="fixed inset-0 z-40 bg-black/60 animate-fade" />
        <D.Content
          className={cn("fixed left-1/2 top-1/2 z-50 w-[min(440px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 rounded-card border border-border bg-surface p-4 shadow-soft outline-none animate-pop", className)}
        >
          <D.Title className="text-[15px] font-semibold text-fg">{title}</D.Title>
          {description ? <D.Description className="mt-1 text-xs text-subtle">{description}</D.Description> : <D.Description className="sr-only">{title}</D.Description>}
          <div className="mt-4">{children}</div>
        </D.Content>
      </D.Portal>
    </D.Root>
  );
}
