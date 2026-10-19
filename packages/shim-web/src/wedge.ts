import type { WedgeScan } from "@hardless/types";

/**
 * A key the wedge presses. `keyCode` and `charCode` are the legacy values many barcode
 * listeners still read (`event.keyCode`, `event.which`), for a US keyboard layout.
 */
export interface WedgeKey {
  key: string;
  code: string;
  shift: boolean;
  /** `keydown`/`keyup` `keyCode`; 0 when there is no key for the character. */
  keyCode: number;
  /** `keypress` `charCode`. */
  charCode: number;
  /** Text the key inserts, if any. */
  text?: string;
}

/** US layout: punctuation keys, their `code`, `keyCode` and shifted characters. */
const PUNCTUATION: [base: string, shifted: string, code: string, keyCode: number][] = [
  ["`", "~", "Backquote", 192],
  ["-", "_", "Minus", 189],
  ["=", "+", "Equal", 187],
  ["[", "{", "BracketLeft", 219],
  ["]", "}", "BracketRight", 221],
  ["\\", "|", "Backslash", 220],
  [";", ":", "Semicolon", 186],
  ["'", '"', "Quote", 222],
  [",", "<", "Comma", 188],
  [".", ">", "Period", 190],
  ["/", "?", "Slash", 191],
];
const SHIFTED_DIGITS = ")!@#$%^&*(";

/** The key that types `char`. */
export function charKey(char: string): WedgeKey {
  const charCode = char.codePointAt(0) ?? 0;
  const plain = { key: char, charCode, text: char };
  if (/^[a-z]$/.test(char)) {
    const upper = char.toUpperCase();
    return { ...plain, code: `Key${upper}`, shift: false, keyCode: upper.charCodeAt(0) };
  }
  if (/^[A-Z]$/.test(char)) {
    return { ...plain, code: `Key${char}`, shift: true, keyCode: char.charCodeAt(0) };
  }
  if (/^[0-9]$/.test(char)) {
    return { ...plain, code: `Digit${char}`, shift: false, keyCode: char.charCodeAt(0) };
  }
  const shiftedDigit = SHIFTED_DIGITS.indexOf(char);
  if (shiftedDigit >= 0) {
    return {
      ...plain,
      code: `Digit${String(shiftedDigit)}`,
      shift: true,
      keyCode: 48 + shiftedDigit,
    };
  }
  if (char === " ") return { ...plain, code: "Space", shift: false, keyCode: 32 };
  for (const [base, shifted, code, keyCode] of PUNCTUATION) {
    if (char === base) return { ...plain, code, shift: false, keyCode };
    if (char === shifted) return { ...plain, code, shift: true, keyCode };
  }
  // Characters with no key on a US layout still arrive as key events with their text.
  return { ...plain, code: "", shift: false, keyCode: 0 };
}

/** The key that ends a scan. */
export function terminatorKey(terminator: "enter" | "tab"): WedgeKey {
  return terminator === "enter"
    ? { key: "Enter", code: "Enter", shift: false, keyCode: 13, charCode: 13 }
    : { key: "Tab", code: "Tab", shift: false, keyCode: 9, charCode: 0 };
}

/** Every key a scan presses, in order. */
export function scanKeys(scan: WedgeScan): WedgeKey[] {
  // One key per code point, the same count the engine uses for the scan's typing time.
  // eslint-disable-next-line @typescript-eslint/no-misused-spread
  const keys = [...scan.code].map(charKey);
  if (scan.terminator !== "none") keys.push(terminatorKey(scan.terminator));
  return keys;
}

const sleep = (ms: number) =>
  new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });

/** Input types that hold typed text (and fire `input` events when typed into). */
const TEXT_INPUTS = new Set(["text", "search", "url", "tel", "email", "password", "number", ""]);

function isTextField(el: Element): el is HTMLInputElement | HTMLTextAreaElement {
  if (el instanceof HTMLTextAreaElement) return true;
  return el instanceof HTMLInputElement && TEXT_INPUTS.has(el.type);
}

/**
 * The keyboard-wedge injector (FR-14, PRD section 8.4): types scans into the focused
 * element the way a USB scanner's key presses arrive, one scan after another.
 *
 * For each character it dispatches `keydown`, `keypress`, `beforeinput`/`input` (in text
 * fields, where it also inserts the character) and `keyup`, honouring `preventDefault()`
 * like a browser does. Enter submits the field's form (implicit submission) or adds a line
 * in a textarea; Tab moves focus to the next field.
 *
 * Limitation: the events are synthetic, so `event.isTrusted` is `false`. Apps that ignore
 * untrusted key events cannot be tested with the wedge in phase 1 (OS-level injection comes
 * in phase 2).
 */
export class WedgeInjector {
  private queue: Promise<void> = Promise.resolve();
  private stopped = false;

  constructor(private readonly doc: Document = document) {}

  /** Types a scan after any scans still typing. Resolves when its last key is up. */
  type(scan: WedgeScan): Promise<void> {
    const run = this.queue.then(() => this.run(scan));
    this.queue = run.catch(() => undefined);
    return run;
  }

  /** Resolves when every scan received so far has been typed. */
  idle(): Promise<void> {
    return this.queue;
  }

  /** Stops typing; scans not yet typed are dropped. */
  stop(): void {
    this.stopped = true;
  }

  private async run(scan: WedgeScan): Promise<void> {
    const keys = scanKeys(scan);
    for (const [i, key] of keys.entries()) {
      if (this.stopped) return;
      if (i > 0 && scan.delayMs > 0) await sleep(scan.delayMs);
      this.press(key);
    }
  }

  /** Where keys go: the focused element, or the body when nothing has focus. */
  private target(): HTMLElement {
    const active = this.doc.activeElement;
    return active instanceof HTMLElement ? active : this.doc.body;
  }

  private press(key: WedgeKey): void {
    const target = this.target();
    let act = target.dispatchEvent(this.keyEvent("keydown", key, key.keyCode, 0));
    if (act && key.charCode !== 0) {
      act = target.dispatchEvent(this.keyEvent("keypress", key, key.charCode, key.charCode));
    }
    if (act) {
      if (key.text !== undefined) this.insert(target, key.text);
      else if (key.key === "Enter") this.enter(target);
      else if (key.key === "Tab") this.tab(target);
    }
    // keyup goes to whatever has focus now (Tab may have moved it).
    this.target().dispatchEvent(this.keyEvent("keyup", key, key.keyCode, 0));
  }

  private keyEvent(type: string, key: WedgeKey, keyCode: number, charCode: number): KeyboardEvent {
    const view = this.doc.defaultView;
    const event = new (view?.KeyboardEvent ?? KeyboardEvent)(type, {
      key: key.key,
      code: key.code,
      shiftKey: key.shift,
      bubbles: true,
      cancelable: true,
      composed: true,
      view,
    });
    // The KeyboardEvent constructor ignores the legacy fields; barcode listeners read them.
    const which = type === "keypress" ? charCode : keyCode;
    Object.defineProperty(event, "keyCode", { get: () => keyCode });
    Object.defineProperty(event, "charCode", { get: () => charCode });
    Object.defineProperty(event, "which", { get: () => which });
    return event;
  }

  /** Inserts typed text into a text field or editable element, with input events. */
  private insert(target: HTMLElement, text: string, inputType = "insertText"): void {
    const editable = isTextField(target) && !target.disabled && !target.readOnly;
    if (!editable && !target.isContentEditable) return;
    const view = this.doc.defaultView;
    const Input = view?.InputEvent ?? InputEvent;
    const init = { inputType, data: text, bubbles: true, composed: true };
    if (!target.dispatchEvent(new Input("beforeinput", { ...init, cancelable: true }))) return;
    if (isTextField(target)) {
      try {
        // Keeps the caret, and goes around the value setter that React watches, so React
        // sees the change in the `input` event as it does for real typing.
        const end = target.selectionEnd ?? target.value.length;
        target.setRangeText(text, target.selectionStart ?? end, end, "end");
      } catch {
        // Types without a selection API (number, email): append through the native setter.
        const proto =
          target instanceof HTMLTextAreaElement
            ? HTMLTextAreaElement.prototype
            : HTMLInputElement.prototype;
        Reflect.set(proto, "value", target.value + text, target);
      }
    } else {
      const selection = this.doc.getSelection();
      const range = selection && selection.rangeCount > 0 ? selection.getRangeAt(0) : null;
      if (!range) return;
      range.deleteContents();
      const node = this.doc.createTextNode(inputType === "insertLineBreak" ? "\n" : text);
      range.insertNode(node);
      range.setStartAfter(node);
      range.collapse(true);
    }
    target.dispatchEvent(new Input("input", init));
  }

  /** Enter: a new line in a textarea, or implicit submission of the field's form. */
  private enter(target: HTMLElement): void {
    if (target instanceof HTMLTextAreaElement || target.isContentEditable) {
      this.insert(target, "\n", "insertLineBreak");
      return;
    }
    if (!(target instanceof HTMLInputElement) || !target.form) return;
    const form = target.form;
    const fields = [...form.elements];
    const button = fields.find(
      (el): el is HTMLButtonElement | HTMLInputElement =>
        (el instanceof HTMLButtonElement && el.type === "submit") ||
        (el instanceof HTMLInputElement && (el.type === "submit" || el.type === "image")),
    );
    // HTML implicit submission: click the default button; with none, submit only when the
    // form has a single text field.
    if (button) {
      if (!button.disabled) button.click();
    } else if (fields.filter(isTextField).length === 1) {
      form.requestSubmit();
    }
  }

  /** Tab: focus the next focusable element in tab order. */
  private tab(target: HTMLElement): void {
    const focusable = [
      ...this.doc.querySelectorAll<HTMLElement>(
        "a[href], button, input, select, textarea, [tabindex], [contenteditable='true']",
      ),
    ].filter(
      (el) =>
        el.tabIndex >= 0 &&
        !(el as HTMLInputElement).disabled &&
        !(el instanceof HTMLInputElement && el.type === "hidden") &&
        el.getClientRects().length > 0,
    );
    // Positive tabindex first (in order), then the rest in document order.
    const ordered = [
      ...focusable.filter((el) => el.tabIndex > 0).sort((a, b) => a.tabIndex - b.tabIndex),
      ...focusable.filter((el) => el.tabIndex === 0),
    ];
    const next = ordered[ordered.indexOf(target) + 1];
    next?.focus();
  }
}
