/** Devices an origin was granted through a picker, kept for the tab. */
export interface Grants {
  has(id: string): boolean;
  add(id: string): void;
  remove(id: string): void;
  all(): string[];
}

/**
 * Grants kept in sessionStorage under `key`, so `getPorts()` and `getDevices()` still find
 * them after a reload (PRD sections 8.1 and 8.2).
 */
export function createGrants(key: string): Grants {
  const read = (): string[] => {
    try {
      const value: unknown = JSON.parse(sessionStorage.getItem(key) ?? "[]");
      return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
    } catch {
      return [];
    }
  };
  const write = (ids: string[]): void => {
    try {
      sessionStorage.setItem(key, JSON.stringify(ids));
    } catch {
      // Storage may be unavailable (sandboxed frames); grants then last for the page only.
    }
  };
  const memory = new Set<string>();
  return {
    has(id) {
      return memory.has(id) || read().includes(id);
    },
    add(id) {
      memory.add(id);
      const ids = read();
      if (!ids.includes(id)) write([...ids, id]);
    },
    remove(id) {
      memory.delete(id);
      write(read().filter((i) => i !== id));
    },
    all() {
      return [...new Set([...read(), ...memory])];
    },
  };
}

/** Serial ports granted with `requestPort()` (sessionStorage key `hardless:grants`). */
export const grants = createGrants("hardless:grants");

/** USB devices granted with `requestDevice()` (sessionStorage key `hardless:usb-grants`). */
export const usbGrants = createGrants("hardless:usb-grants");
