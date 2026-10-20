import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import "./style.css";

/**
 * Development only: replace Web Serial with Hardless, so the app prints to the emulated
 * printer instead of a real one. Production builds never include the shim.
 * Set VITE_HARDLESS=off to use real hardware in development.
 */
async function start(): Promise<void> {
  if (import.meta.env.DEV && import.meta.env.VITE_HARDLESS !== "off") {
    const { install } = await import("@hardless/shim-web");
    // The bridge on the host the app came from: wss:// on 8443 from an https:// page, which
    // may not open ws:// (mixed content).
    const secure = window.location.protocol === "https:";
    const bridge = secure
      ? `wss://${window.location.hostname}:8443/bridge`
      : `ws://${window.location.hostname}:8080/bridge`;
    install({
      mode: "server",
      url: (import.meta.env.VITE_HARDLESS_URL as string | undefined) ?? bridge,
      // On a phone or tablet the app is opened at a LAN address, not localhost.
      allowAnyOrigin: true,
    });
  }
  const root = document.getElementById("root");
  if (!root) throw new Error("missing #root element");
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

void start();
