import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

/**
 * `HTTPS=1 pnpm dev` serves the app over HTTPS with the certificate the Hardless server made
 * (start Hardless first). Devices that trust the Hardless certificate trust the app too, so
 * an iPhone or iPad can open it at the computer's LAN address (`--host`).
 */
function hardlessCertDir(): string {
  if (process.env.HARDLESS_CERT_DIR) return process.env.HARDLESS_CERT_DIR;
  if (process.platform === "win32" && process.env.APPDATA) {
    return join(process.env.APPDATA, "hardless", "certs");
  }
  if (process.platform === "darwin") {
    return join(homedir(), "Library", "Application Support", "hardless", "certs");
  }
  const data = process.env.XDG_DATA_HOME ?? join(homedir(), ".local", "share");
  return join(data, "hardless", "certs");
}

function https() {
  if (process.env.HTTPS !== "1") return undefined;
  const dir = hardlessCertDir();
  try {
    return {
      cert: readFileSync(join(dir, "hardless-server.pem")),
      key: readFileSync(join(dir, "hardless-server-key.pem")),
    };
  } catch {
    throw new Error(
      `HTTPS=1 needs the Hardless certificate in ${dir}. Start Hardless first (hardless start).`,
    );
  }
}

const tls = https();
const secure = tls ? { https: tls } : {};

export default defineConfig({
  plugins: [react()],
  server: { port: 5173, strictPort: true, ...secure },
  preview: { port: 5173, strictPort: true, ...secure },
});
