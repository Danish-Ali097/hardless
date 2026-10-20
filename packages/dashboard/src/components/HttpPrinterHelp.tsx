import type { DeviceSummary } from "@hardless/types";
import type { ServerInfo } from "../api.js";

/** The host the dashboard was opened on: the address other devices on the LAN use too. */
function currentHost(): string {
  return typeof window === "undefined" ? "localhost" : window.location.hostname;
}

function Address({ label, url }: { label: string; url: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs text-muted">{label}</span>
      <code className="block overflow-x-auto rounded bg-panel px-2 py-1 font-mono text-xs whitespace-nowrap">
        {url}
      </code>
    </div>
  );
}

/**
 * FR-33 and FR-35: where an app sends prints to an HTTP printer, why an HTTPS app needs the
 * https:// address (mixed content), and how to trust the Hardless certificate.
 */
export function HttpPrinterHelp({
  printer,
  info,
  isDefault,
}: {
  printer: DeviceSummary;
  info: ServerInfo | null;
  isDefault: boolean;
}) {
  const host = currentHost();
  const httpPort = info?.httpPort ?? 8080;
  const https = info?.https;
  const http = `http://${host}:${String(httpPort)}`;
  const secure = https?.enabled && https.port ? `https://${host}:${String(https.port)}` : null;
  const epos = `/cgi-bin/epos/service.cgi?devid=${encodeURIComponent(printer.id)}&timeout=10000`;
  const webprnt = isDefault
    ? "/StarWebPRNT/SendMessage"
    : `/p/${encodeURIComponent(printer.id)}/StarWebPRNT/SendMessage`;
  const certificate = `${http}${https?.certificate ?? "/hardless-ca.crt"}`;
  const covered = https?.hosts.includes(host) ?? true;

  return (
    <section aria-labelledby="http-help" className="mt-4 flex flex-col gap-3 text-sm">
      <h2 id="http-help" className="text-sm font-semibold">
        Printing over HTTP
      </h2>
      <p className="text-muted">
        Point your app&apos;s printer address here. Both work from any browser, including Safari on
        iPhone and iPad.
      </p>
      <div className="grid gap-2 md:grid-cols-2">
        <Address label="ePOS-Print-compatible" url={`${secure ?? http}${epos}`} />
        <Address label="WebPRNT-compatible" url={`${secure ?? http}${webprnt}`} />
      </div>
      {secure && (
        <p className="text-xs text-muted">
          Without HTTPS: <code className="font-mono">{http}</code> instead.
        </p>
      )}

      <details className="rounded-md border border-line bg-raised p-3">
        <summary className="cursor-pointer font-medium">
          My app is on https:// and printing fails
        </summary>
        <div className="mt-2 flex flex-col gap-2 text-muted">
          <p>
            <strong className="text-ink">Mixed content.</strong> A page loaded over https:// may not
            call an http:// address: the browser blocks it before it leaves the page, the same as
            with a real printer. Use the https:// address above.
          </p>
          <p>
            <strong className="text-ink">Untrusted certificate.</strong> The https:// address fails
            until this device trusts the Hardless certificate. Trust it once per device; later
            certificates are trusted automatically.
          </p>
          {https?.enabled ? (
            <>
              <a
                href={certificate}
                download="hardless-ca.crt"
                className="self-start rounded-md bg-ink px-3 py-1.5 font-medium text-counter hover:opacity-90"
              >
                Download the Hardless certificate
              </a>
              <ul className="list-disc space-y-1 pl-5">
                <li>
                  <strong className="text-ink">iPhone or iPad:</strong> open {certificate} in
                  Safari, then Settings, Profile Downloaded, Install. Then Settings, General, About,
                  Certificate Trust Settings, and turn on Hardless Local CA.
                </li>
                <li>
                  <strong className="text-ink">Android:</strong> Settings, Security, Encryption and
                  credentials, Install a certificate, CA certificate.
                </li>
                <li>
                  <strong className="text-ink">macOS:</strong> open the file, add it to the System
                  keychain, then set it to Always Trust.
                </li>
                <li>
                  <strong className="text-ink">Windows:</strong> open the file, Install Certificate,
                  Local Machine, Trusted Root Certification Authorities.
                </li>
              </ul>
              {!covered && (
                <p role="alert" className="text-danger">
                  This address ({host}) is not in the certificate, which covers{" "}
                  {https.hosts.join(", ")}. Start Hardless with <code>--host {host}</code> or set{" "}
                  <code>HARDLESS_CERT_HOSTS={host}</code>.
                </p>
              )}
            </>
          ) : (
            <p role="alert" className="text-danger">
              HTTPS is off{https?.error ? `: ${https.error}` : ""}.
            </p>
          )}
        </div>
      </details>
    </section>
  );
}
