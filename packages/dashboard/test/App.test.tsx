import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { App } from "../src/App.js";

describe("dashboard shell", () => {
  it("renders the empty state", () => {
    const html = renderToString(<App />);
    expect(html).toContain("Hardless");
    expect(html).toContain("No devices yet");
  });

  it("NFR-13: has light and dark theme styles", () => {
    const html = renderToString(<App />);
    expect(html).toContain("bg-white");
    expect(html).toContain("dark:bg-neutral-950");
  });
});
