// Prints an ESC/POS file to a running Hardless server through the bridge, the way a web app
// using Web Serial would. Handy for trying the dashboard without an app:
//
//   node scripts/print-sample.mjs [file.bin] [deviceId] [serverUrl]
//
// Defaults: fixtures/escpos/33-full-receipt.bin, printer-1, http://127.0.0.1:8080.
import { readFileSync } from "node:fs";

const file = process.argv[2] ?? new URL("../fixtures/escpos/33-full-receipt.bin", import.meta.url);
const deviceId = process.argv[3] ?? "printer-1";
const server = new URL(process.argv[4] ?? process.env.HARDLESS_URL ?? "http://127.0.0.1:8080");
const token = process.env.HARDLESS_TOKEN;

const url = new URL("/bridge", server);
url.protocol = server.protocol === "https:" ? "wss:" : "ws:";
if (token) url.searchParams.set("token", token);

const data = readFileSync(file).toString("base64");
const socket = new WebSocket(url);
const send = (message) => socket.send(JSON.stringify({ v: 1, ...message }));

socket.addEventListener("open", () => {
  send({
    type: "hello",
    id: "hello",
    origin: "print-sample",
    shimVersion: "0.0.0",
    apis: ["serial"],
  });
  send({ type: "open", id: "open", deviceId, options: { baudRate: 9600 } });
});
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.type === "error") {
    console.error(`${deviceId}: ${message.message}`);
    process.exitCode = 1;
    socket.close();
  } else if (message.type === "ack" && message.replyTo === "open") {
    send({ type: "write", id: "write", deviceId, data });
  } else if (message.type === "ack" && message.replyTo === "write") {
    console.log(`printed ${String(Buffer.from(data, "base64").length)} bytes to ${deviceId}`);
    socket.close();
  }
});
socket.addEventListener("error", () => {
  console.error(`cannot reach ${url.href}; is \`hardless start\` running?`);
  process.exitCode = 1;
});
