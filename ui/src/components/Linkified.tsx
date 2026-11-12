import { Fragment } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

const URL_RE = /\bhttps?:\/\/[^\s<>"']+[^\s<>"'.,;:!?)\]]/gi;

/** Text with http(s) links that open in the default browser. */
export function Linkified({ text }: { text: string }) {
  const parts: (string | { url: string })[] = [];
  let last = 0;
  for (const match of text.matchAll(URL_RE)) {
    const i = match.index ?? 0;
    if (i > last) parts.push(text.slice(last, i));
    parts.push({ url: match[0] });
    last = i + match[0].length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return (
    <>
      {parts.map((p, i) =>
        typeof p === "string" ? (
          <Fragment key={i}>{p}</Fragment>
        ) : (
          <a
            key={i}
            href={p.url}
            onClick={(e) => {
              e.preventDefault();
              void openUrl(p.url);
            }}
            className="cursor-pointer underline decoration-current/40 underline-offset-2 hover:decoration-current"
          >
            {p.url}
          </a>
        ),
      )}
    </>
  );
}
