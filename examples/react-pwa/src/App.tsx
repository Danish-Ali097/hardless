import { useEffect, useMemo, useState, type FormEvent } from "react";
import { DRAWER_KICK, money, receipt, total, type Line, type Sale } from "./escpos.js";
import { defaultAddress, eposDrawerKick, eposReceipt, printOverNetwork } from "./epos.js";
import { ReceiptPrinter } from "./printer.js";

const ORDER: Line[] = [
  { name: "Flat white", quantity: 2, price: 450 },
  { name: "Croissant", quantity: 1, price: 350 },
];

/** What the till sells, by barcode. */
const CATALOGUE: Record<string, { name: string; price: number }> = {
  "4006381333931": { name: "Flat white", price: 450 },
  "8964000123456": { name: "Croissant", price: 350 },
  "5012345678900": { name: "Orange juice", price: 300 },
};

type Notice = { tone: "ok" | "error"; text: string } | null;
type Route = "serial" | "network";

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

/** Adds one of `item` to the order. */
function add(order: Line[], item: { name: string; price: number }): Line[] {
  const existing = order.find((line) => line.name === item.name);
  if (existing) {
    return order.map((line) =>
      line === existing ? { ...line, quantity: line.quantity + 1 } : line,
    );
  }
  return [...order, { ...item, quantity: 1 }];
}

export function App() {
  const printer = useMemo(() => new ReceiptPrinter(), []);
  const [route, setRoute] = useState<Route>("serial");
  const [address, setAddress] = useState(() => defaultAddress(window.location));
  const [connected, setConnected] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);
  const [order, setOrder] = useState<Line[]>(ORDER);
  const [barcode, setBarcode] = useState("");

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
      const sale = { lines: order, payment };
      if (route === "network") {
        await printOverNetwork(address, eposReceipt(sale));
      } else {
        await printer.write(receipt(sale));
        const problem = failure(await printer.status());
        if (problem) throw new Error(problem);
      }
      setNotice({
        tone: "ok",
        text: payment === "cash" ? "Receipt printed. Drawer opened." : "Receipt printed.",
      });
    });

  const openDrawer = () =>
    run(async () => {
      if (route === "network") {
        await printOverNetwork(address, eposDrawerKick());
      } else {
        await printer.write(DRAWER_KICK);
      }
      setNotice({ tone: "ok", text: "Drawer opened." });
    });

  // A barcode scanner types the code and presses Enter, which submits this form.
  const scan = (event: FormEvent) => {
    event.preventDefault();
    const code = barcode.trim();
    setBarcode("");
    if (!code) return;
    const item = CATALOGUE[code];
    if (item) {
      setOrder((current) => add(current, item));
      setNotice({ tone: "ok", text: `Added ${item.name}.` });
    } else {
      setNotice({ tone: "error", text: `Unknown barcode ${code}.` });
    }
  };

  const ready = route === "network" || connected;
  return (
    <main className="pos">
      <header>
        <h1>Corner Café</h1>
        <span className={ready ? "printer on" : "printer"} role="status">
          {route === "network" ? "Network printer" : connected ? "Printer connected" : "No printer"}
        </span>
      </header>

      <form className="scan" onSubmit={scan}>
        <label htmlFor="barcode">Scan or type a barcode</label>
        <div>
          <input
            id="barcode"
            value={barcode}
            onChange={(e) => {
              setBarcode(e.target.value);
            }}
            autoComplete="off"
            inputMode="numeric"
          />
          <button type="submit">Add</button>
        </div>
      </form>

      <section className="order" aria-label="Order">
        <ul>
          {order.map((line) => (
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
          <span>{money(total(order))}</span>
        </p>
      </section>

      <fieldset className="route">
        <legend>Printer</legend>
        <label>
          <input
            type="radio"
            name="route"
            checked={route === "serial"}
            onChange={() => {
              setRoute("serial");
            }}
          />
          USB or serial (Web Serial)
        </label>
        <label>
          <input
            type="radio"
            name="route"
            checked={route === "network"}
            onChange={() => {
              setRoute("network");
            }}
          />
          Network (ePOS-Print)
        </label>
        {route === "network" && (
          <label className="address">
            Printer address
            <input
              value={address}
              onChange={(e) => {
                setAddress(e.target.value);
              }}
              spellCheck={false}
            />
          </label>
        )}
      </fieldset>

      {notice && (
        <p className={`notice ${notice.tone}`} role={notice.tone === "error" ? "alert" : "status"}>
          {notice.text}
        </p>
      )}

      <div className="actions">
        {!ready ? (
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
