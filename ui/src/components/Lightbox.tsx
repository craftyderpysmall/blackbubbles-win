import { useEffect, useState } from "react";
import { Dialog as Theater } from "radix-ui";
import { ChevronLeft, ChevronRight, Download, X } from "lucide-react";
import { blobUrl, saveAs, type BlobKind } from "@/lib/bb";
import { useApp } from "@/lib/app";
import { Button, IconButton, Spinner } from "./ui";

/** Full-screen image viewer with keyboard navigation and Save As. */
export function Lightbox({
  kind,
  id,
  name,
  placeholder,
  caption,
  onClose,
  onPrev,
  onNext,
}: {
  kind: BlobKind;
  id: string;
  name: string;
  placeholder?: string;
  caption?: React.ReactNode;
  onClose: () => void;
  onPrev?: () => void;
  onNext?: () => void;
}) {
  const { toast } = useApp();
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  useEffect(() => setLoaded(false), [id]);
  const [opener] = useState(() => document.activeElement instanceof HTMLElement ? document.activeElement : null);

  const save = () => {
    setSaving(true);
    void saveAs(kind, id, name, toast).finally(() => setSaving(false));
  };

  return (
    <Theater.Root open onOpenChange={(open) => !open && onClose()}>
      <Theater.Portal>
        <Theater.Content
          aria-describedby={undefined}
          onCloseAutoFocus={(event) => { event.preventDefault(); opener?.focus(); }}
          onKeyDown={(event) => {
            if (event.key === "ArrowLeft") onPrev?.();
            else if (event.key === "ArrowRight") onNext?.();
          }}
          className="fixed inset-0 z-40 flex flex-col bg-deep outline-none animate-fade"
        >
          <div className="flex shrink-0 items-center gap-2 border-b border-border bg-surface px-3 py-2 sm:px-5">
            <Theater.Title className="min-w-0 flex-1 truncate text-[13px] text-body">{caption || name}</Theater.Title>
            <Button variant="outline" icon={Download} busy={saving} onClick={save}>Save As…</Button>
            <Theater.Close asChild>
              <IconButton icon={X} label="Close" />
            </Theater.Close>
          </div>
          <div className="relative flex min-h-0 flex-1 items-center justify-center px-12 py-4 sm:px-16 sm:py-6" onClick={onClose}>
            {placeholder && !loaded && <img src={placeholder} alt="" className="absolute max-h-full max-w-full object-contain opacity-60 blur-sm" />}
            {!loaded && !placeholder && <Spinner className="absolute" />}
            <img
              key={id}
              src={blobUrl(kind, id)}
              alt={name}
              onLoad={() => setLoaded(true)}
              onClick={(event) => event.stopPropagation()}
              className="relative max-h-full max-w-full rounded-control object-contain transition-opacity duration-150"
              style={{ opacity: loaded ? 1 : 0 }}
            />
            {onPrev && (
              <IconButton icon={ChevronLeft} label="Previous" variant="outline" className="absolute left-2 sm:left-4" onClick={(event) => { event.stopPropagation(); onPrev(); }} />
            )}
            {onNext && (
              <IconButton icon={ChevronRight} label="Next" variant="outline" className="absolute right-2 sm:right-4" onClick={(event) => { event.stopPropagation(); onNext(); }} />
            )}
          </div>
        </Theater.Content>
      </Theater.Portal>
    </Theater.Root>
  );
}
