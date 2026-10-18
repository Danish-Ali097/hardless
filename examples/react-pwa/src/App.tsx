import { useEffect, useMemo, useState } from "react";
import { DRAWER_KICK, money, receipt, total, type Line, type Sale } from "./escpos.js";
import { ReceiptPrinter } from "./printer.js";

const ORDER: Line[] = [
  { name: "Flat white", quantity: 2, price: 450 },
  { name: "Croissant", quantity: 1, price: 350 },
];

type Notice = { tone: "ok" | "error"; text: string } | null;

/** Why a print failed, from the printer's own status. */
function failure(status: {
  online: boolean;
  paperOut: boolean;
  coverOpen: boolean;
}): string | null {
  if (status.paperOut) return "The receipt did not print: the printer is out of paper.";
  if (status.coverOpen) return "The receipt did not print: the printer cover is open.";
  if (!status.online) return "The receipt did not print: the printer is offline.";
  return null;
}

export function App() {
  const printer = useMemo(() => new ReceiptPrinter(), []);
  const [connected, setConnected] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);

  useEffect(() => {
    printer.onDisconnect = () => {
      setConnected(false);
      setNotice({ tone: "error", text: "The printer was disconnected." });
    };
    // Reconnect to a printer chosen before (no click needed for getPorts()).
    printer.reconnect().then(setConnected, () => undefined);
  }, [printer]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setNotice(null);
    try {
      await action();
    } catch (error) {
      setNotice({ tone: "error", text: error instanceof Error ? error.message : String(error) });
    } finally {
      setBusy(false);
    }
  };

  const connect = () =>
    run(async () => {
      await printer.connect();
      setConnected(true);
      setNotice({ tone: "ok", text: "Printer connected." });
    });

  const pay = (payment: Sale["payment"]) =>
    run(async () => {
      await printer.write(receipt({ lines: ORDER, payment }));
      const problem = failure(await printer.status());
      if (problem) throw new Error(problem);
      setNotice({
        tone: "ok",
        text: payment === "cash" ? "Receipt printed. Drawer opened." : "Receipt printed.",
      });
    });

  const openDrawer = () =>
    run(async () => {
      await printer.write(DRAWER_KICK);
      setNotice({ tone: "ok", text: "Drawer opened." });
    });

  return (
    <main className="pos">
      <header>
        <h1>Corner Café</h1>
        <span className={connected ? "printer on" : "printer"} role="status">
          {connected ? "Printer connected" : "No printer"}
        </span>
      </header>

      <section className="order" aria-label="Order">
        <ul>
          {ORDER.map((line) => (
            <li key={line.name}>
              <span>
                {line.quantity} × {line.name}
              </span>
              <span>{money(line.quantity * line.price)}</span>
            </li>
          ))}
        </ul>
        <p className="total">
          <span>Total</span>
          <span>{money(total(ORDER))}</span>
        </p>
      </section>

      {notice && (
        <p className={`notice ${notice.tone}`} role={notice.tone === "error" ? "alert" : "status"}>
          {notice.text}
        </p>
      )}

      <div className="actions">
        {!connected ? (
          <button type="button" className="primary" onClick={() => void connect()} disabled={busy}>
            Connect printer
          </button>
        ) : (
          <>
            <button
              type="button"
              className="primary"
              onClick={() => void pay("cash")}
              disabled={busy}
            >
              Pay cash
            </button>
            <button type="button" onClick={() => void pay("card")} disabled={busy}>
              Pay card
            </button>
            <button type="button" onClick={() => void openDrawer()} disabled={busy}>
              Open drawer
            </button>
          </>
        )}
      </div>
    </main>
  );
}
