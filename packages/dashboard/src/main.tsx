import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { connect, load } from "./actions.js";
import { App } from "./App.js";
import "./index.css";

const root = document.getElementById("root");
if (root === null) throw new Error("missing #root element");

// Show what the server has straight away; the stream then keeps it live.
void load();
connect();

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
