import { describe, expect, it } from "vitest";
import { applyEvent, firstDevice, MAX_JOBS, needsRefresh, visibleJobs } from "../src/store.js";
import { drawer, job, printer, stateWith } from "./fixtures.js";

describe("dashboard store", () => {
  it("FR-23: removes devices and moves the selection when the selected one goes", () => {
    const state = stateWith([printer(), drawer()], { selected: "drawer-1" });
    const next = {
      ...state,
      ...applyEvent(state, { type: "device:removed", deviceId: "drawer-1", at: 5 }),
    };
    expect(Object.keys(next.devices)).toEqual(["printer-1"]);
    expect(next.selected).toBe("printer-1");
  });

  it("FR-23: applies live state changes", () => {
    const state = stateWith([printer()]);
    const next = applyEvent(state, {
      type: "device:state",
      deviceId: "printer-1",
      state: { connected: false },
      at: 9,
    });
    expect(next.devices?.["printer-1"]?.state).toEqual({ connected: false });
  });

  it("FR-23: reloads a device summary on add and on state change (faults are not in the event)", () => {
    expect(
      needsRefresh({ type: "device:added", deviceId: "x", kind: "printer-escpos", at: 0 }),
    ).toBe("x");
    expect(needsRefresh({ type: "device:state", deviceId: "x", state: {}, at: 0 })).toBe("x");
    expect(needsRefresh({ type: "device:removed", deviceId: "x", at: 0 })).toBeNull();
  });

  it("FR-24: new receipts go on top, without duplicates, up to a limit", () => {
    let state = stateWith([printer()]);
    for (let id = 1; id <= MAX_JOBS + 5; id++) {
      state = {
        ...state,
        ...applyEvent(state, { type: "device:output", deviceId: "printer-1", job: job(id) }),
      };
    }
    state = {
      ...state,
      ...applyEvent(state, {
        type: "device:output",
        deviceId: "printer-1",
        job: job(MAX_JOBS + 5),
      }),
    };
    const jobs = state.jobs["printer-1"] ?? [];
    expect(jobs).toHaveLength(MAX_JOBS);
    expect(jobs[0]?.id).toBe(MAX_JOBS + 5);
  });

  it("FR-24: clearing hides earlier receipts, even after job ids restart", () => {
    const state = stateWith([printer()], {
      jobs: { "printer-1": [job(1, 900), job(2, 500), job(1, 100)] },
      clearedAt: { "printer-1": 500 },
    });
    expect(visibleJobs(state, "printer-1").map((j) => j.at)).toEqual([900]);
  });

  it("selects printers before other devices", () => {
    expect(firstDevice(stateWith([drawer(), printer()]).devices)).toBe("printer-1");
    const http = printer({ id: "local_printer" }, { transport: "http" });
    expect(firstDevice(stateWith([http, drawer(), printer()]).devices)).toBe("printer-1");
    expect(firstDevice(stateWith([http, drawer()]).devices)).toBe("local_printer");
    expect(firstDevice({})).toBeNull();
  });
});
