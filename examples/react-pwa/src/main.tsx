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
    install({
      mode: "server",
      url:
        (import.meta.env.VITE_HARDLESS_URL as string | undefined) ?? "ws://localhost:8080/bridge",
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
