/**
 * Ports this origin was granted with `requestPort()`. Kept in sessionStorage under
 * `hardless:grants`, so `getPorts()` still finds them after a reload (PRD section 8.1).
 */
const KEY = "hardless:grants";

function read(): string[] {
  try {
    const value: unknown = JSON.parse(sessionStorage.getItem(KEY) ?? "[]");
    return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
  } catch {
    return [];
  }
}

function write(ids: string[]): void {
  try {
    sessionStorage.setItem(KEY, JSON.stringify(ids));
  } catch {
    // Storage may be unavailable (sandboxed frames); grants then last for the page only.
  }
}

const memory = new Set<string>();

export const grants = {
  has(id: string): boolean {
    return memory.has(id) || read().includes(id);
  },
  add(id: string): void {
    memory.add(id);
    const ids = read();
    if (!ids.includes(id)) write([...ids, id]);
  },
  remove(id: string): void {
    memory.delete(id);
    write(read().filter((i) => i !== id));
  },
  all(): string[] {
    return [...new Set([...read(), ...memory])];
  },
};
