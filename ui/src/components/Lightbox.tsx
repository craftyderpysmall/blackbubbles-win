import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { ChevronLeft, ChevronRight, Download, X } from "lucide-react";
import { blobUrl, saveAs, type BlobKind } from "@/lib/bb";
import { useApp } from "@/lib/app";
import { Button, IconButton, Spinner } from "./ui";

/** Full-screen image viewer: arrow keys step, Esc closes, Save writes to Downloads. */
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
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowLeft") onPrev?.();
      else if (e.key === "ArrowRight") onNext?.();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, onPrev, onNext]);

  const save = () => {
    setSaving(true);
    void saveAs(kind, id, name, toast).finally(() => setSaving(false));
  };

  return createPortal(
    <div className="fixed inset-0 z-40 flex flex-col bg-bg/95 animate-fade" onClick={onClose}>
      <div className="flex items-center gap-2 p-4" onClick={(e) => e.stopPropagation()}>
        <div className="min-w-0 flex-1 truncate text-xs text-muted">{caption}</div>
        <Button variant="primary" icon={Download} busy={saving} onClick={save}>
          Save As…
        </Button>
        <IconButton icon={X} label="Close" onClick={onClose} />
      </div>
      <div className="relative flex min-h-0 flex-1 items-center justify-center px-16 pb-8">
        {placeholder && !loaded && <img src={placeholder} alt="" className="absolute max-h-full max-w-full scale-[0.98] object-contain opacity-60 blur-sm" />}
        {!loaded && !placeholder && <Spinner className="absolute" />}
        <img
          key={id}
          src={blobUrl(kind, id)}
          alt={name}
          onLoad={() => setLoaded(true)}
          onClick={(e) => e.stopPropagation()}
          className="relative max-h-full max-w-full rounded-control object-contain shadow-soft transition-[opacity,transform] duration-300 ease-decel"
          style={{ opacity: loaded ? 1 : 0, transform: loaded ? "scale(1)" : "scale(0.98)" }}
        />
        {onPrev && (
          <IconButton icon={ChevronLeft} label="Previous" className="absolute left-4 border border-border bg-surface" onClick={(e) => (e.stopPropagation(), onPrev())} />
        )}
        {onNext && (
          <IconButton icon={ChevronRight} label="Next" className="absolute right-4 border border-border bg-surface" onClick={(e) => (e.stopPropagation(), onNext())} />
        )}
      </div>
    </div>,
    document.body,
  );
}
