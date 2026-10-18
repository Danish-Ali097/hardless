import type { Job } from "@hardless/types";
import { useEffect, useRef, useState } from "react";
import { renderUrl } from "../api.js";
import { hexDump } from "../devices.js";

/**
 * One receipt from the output tray, rendered by the server's renderer (FR-24) into a shadow
 * root so its styles stay separate from the dashboard's.
 */
export function Receipt({
  deviceId,
  job,
  width,
  showUnknown,
}: {
  deviceId: string;
  job: Job;
  width: 58 | 80;
  showUnknown: boolean;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const url = renderUrl(deviceId, job.id, width, showUnknown);

  useEffect(() => {
    let cancelled = false;
    const target = host.current;
    if (!target) return;
    fetch(url)
      .then((r) =>
        r.ok ? r.text() : Promise.reject(new Error(`render failed (${String(r.status)})`)),
      )
      .then((html) => {
        if (cancelled) return;
        const doc = new DOMParser().parseFromString(html, "text/html");
        const style = doc.querySelector("style")?.textContent ?? "";
        const receipt = doc.querySelector(".hl-receipt")?.outerHTML ?? "";
        const root = target.shadowRoot ?? target.attachShadow({ mode: "open" });
        // The renderer escapes all receipt text, so this HTML is safe to insert.
        root.innerHTML = `<style>${style}</style>${receipt}`;
        setFailed(null);
      })
      .catch((error: unknown) => {
        if (!cancelled) setFailed(error instanceof Error ? error.message : String(error));
      });
    return () => {
      cancelled = true;
    };
  }, [url]);

  const bytes = job.bytesHex.length / 2;
  return (
    <figure className="flex flex-col items-center gap-2">
      <div ref={host} aria-label={`Receipt ${String(job.id)}: ${job.text}`} role="img" />
      {failed && <p className="text-sm text-danger">{failed}</p>}
      <figcaption className="flex w-full max-w-md items-baseline justify-between gap-3 text-xs text-muted">
        <span>
          Receipt {job.id}, {bytes} {bytes === 1 ? "byte" : "bytes"}
        </span>
        <a href={url} download={`receipt-${String(job.id)}.html`} className="hover:text-ink">
          Download HTML
        </a>
      </figcaption>
      <details className="w-full max-w-md text-xs">
        <summary className="cursor-pointer text-muted hover:text-ink">Raw bytes</summary>
        <pre className="mt-2 overflow-x-auto rounded-md bg-panel p-3 font-mono leading-relaxed">
          {hexDump(job.bytesHex)}
        </pre>
      </details>
    </figure>
  );
}
