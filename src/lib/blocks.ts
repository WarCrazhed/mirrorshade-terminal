import type { IDecoration, IMarker, Terminal } from "@xterm/xterm";

/**
 * Warp-style command blocks built from shell integration sequences
 * (see src-tauri/shell/mirrorshade.zsh):
 *   OSC 133;A prompt start · 133;B input start · 133;C output start · 133;D;<exit>
 *   OSC 7 cwd · OSC 6973;<nonce>;cmd|path;<base64>
 */

export type ShellState = "unknown" | "prompt" | "input" | "running";

export interface Block {
  id: number;
  prompt: IMarker;
  input?: IMarker;
  inputX: number;
  output?: IMarker;
  end?: IMarker;
  command: string;
  cwd: string;
  exitCode: number | null;
  startedAt: number;
  endedAt: number;
  decorations: IDecoration[];
}

export interface BlockInfo {
  command: string;
  output: string;
  exitCode: number | null;
  cwd: string;
}

const OSC_CUSTOM = 6973;

function decodeB64(b64: string): string {
  try {
    const bin = atob(b64);
    const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
    return new TextDecoder().decode(bytes);
  } catch {
    return "";
  }
}

function fmtDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60_000);
  return `${m}m${Math.round((ms % 60_000) / 1000)}s`;
}

export class BlockTracker {
  blocks: Block[] = [];
  state: ShellState = "unknown";
  cwd = "";
  pathEnv = "";
  selected: Block | undefined;

  private current: Block | undefined;
  private nextId = 1;
  private selectionDeco: IDecoration | undefined;

  constructor(
    private term: Terminal,
    private nonce: string,
    private onDone: (b: Block) => void,
    private onChange: (state: ShellState, cwd: string, command: string) => void = () => {},
    private selectionColor: () => string = () => "#182033",
  ) {
    term.parser.registerOscHandler(133, (data) => this.on133(data));
    term.parser.registerOscHandler(7, (data) => {
      try {
        const u = new URL(data);
        if (u.protocol === "file:") {
          this.cwd = decodeURIComponent(u.pathname);
          this.emit();
        }
      } catch {
        /* ignore malformed */
      }
      return true;
    });
    term.parser.registerOscHandler(OSC_CUSTOM, (data) => {
      const [nonce, key, b64] = data.split(";");
      // Program output could print this sequence too; only trust our shell.
      if (nonce !== this.nonce || !b64) return true;
      const value = decodeB64(b64);
      if (key === "cmd" && this.current) this.current.command = value;
      if (key === "path") this.pathEnv = value;
      return true;
    });
  }

  private emit() {
    this.onChange(this.state, this.cwd, this.state === "running" ? (this.current?.command ?? "") : "");
  }

  private marker(): IMarker | undefined {
    return this.term.registerMarker(0);
  }

  private on133(data: string): boolean {
    const [kind, arg] = data.split(";");
    const buf = this.term.buffer.active;
    switch (kind) {
      case "A": {
        const prompt = this.marker();
        if (!prompt) break;
        const b: Block = {
          id: this.nextId++,
          prompt,
          inputX: 0,
          command: "",
          cwd: this.cwd,
          exitCode: null,
          startedAt: 0,
          endedAt: 0,
          decorations: [],
        };
        prompt.onDispose(() => this.forget(b));
        this.current = b;
        this.state = "prompt";
        break;
      }
      case "B":
        if (this.current) {
          this.current.input = this.marker();
          this.current.inputX = buf.cursorX;
          this.state = "input";
        }
        break;
      case "C":
        if (this.current) {
          if (!this.current.command) this.current.command = this.inputText(this.current) ?? "";
          this.current.output = this.marker();
          this.current.startedAt = performance.now();
          this.current.cwd = this.cwd;
          this.state = "running";
          this.emit();
        }
        break;
      case "D": {
        const b = this.current;
        if (b && this.state === "running") {
          b.end = this.marker();
          b.exitCode = Number.parseInt(arg ?? "0", 10) || 0;
          b.endedAt = performance.now();
          this.blocks.push(b);
          this.decorate(b);
          this.onDone(b);
        }
        this.current = undefined;
        this.state = "unknown";
        this.emit();
        break;
      }
    }
    return true;
  }

  private forget(b: Block) {
    this.blocks = this.blocks.filter((x) => x !== b);
    if (this.selected === b) this.select(undefined);
  }

  private decorate(b: Block) {
    if (!b.end) return;
    const ok = b.exitCode === 0;
    const height = Math.max(1, b.end.line - b.prompt.line);
    const bar = this.term.registerDecoration({ marker: b.prompt, x: 0, width: 1, height, layer: "top" });
    bar?.onRender((el) => {
      el.classList.add("ms-bar", ok ? "ok" : "err");
      el.title = ok ? "Clic: seleccionar bloque" : `Salió con código ${b.exitCode} · clic para seleccionar`;
      el.onclick = () => this.select(b);
    });
    if (bar) b.decorations.push(bar);

    const ms = Math.round(b.endedAt - b.startedAt);
    if (!ok || ms >= 1000) {
      const label = ok ? `✓ ${fmtDuration(ms)}` : `✗ ${b.exitCode} · ${fmtDuration(ms)}`;
      const badge = this.term.registerDecoration({
        marker: b.prompt,
        anchor: "right",
        x: 0,
        width: label.length + 2,
        layer: "top",
      });
      badge?.onRender((el) => {
        el.classList.add("ms-badge", ok ? "ok" : "err");
        el.textContent = label;
      });
      if (badge) b.decorations.push(badge);
    }
  }

  /** Text the user has typed since the prompt ended, up to the cursor. */
  inputText(b: Block | undefined = this.current): string | undefined {
    if (!b?.input || b.input.isDisposed) return undefined;
    const buf = this.term.buffer.active;
    if (buf.type !== "normal") return undefined;
    const cursorLine = buf.baseY + buf.cursorY;
    if (cursorLine < b.input.line) return undefined;
    let s = "";
    for (let y = b.input.line; y <= cursorLine; y++) {
      const line = buf.getLine(y);
      if (!line) break;
      const from = y === b.input.line ? b.inputX : 0;
      const to = y === cursorLine ? buf.cursorX : undefined;
      s += line.translateToString(false, from, to);
    }
    return s;
  }

  /** Current input if the shell is waiting at a prompt and the cursor is at the end of it. */
  currentInput(): string | undefined {
    if (this.state !== "input") return undefined;
    const buf = this.term.buffer.active;
    const line = buf.getLine(buf.baseY + buf.cursorY);
    // Cursor must be at the end of the input. Text far to the right (3+
    // spaces away) is a right prompt (RPROMPT), not part of the input.
    const rest = line?.translateToString(false, buf.cursorX) ?? "";
    if (!/^\s*$/.test(rest) && !/^\s{3,}\S/.test(rest)) return undefined;
    return this.inputText();
  }

  outputText(b: Block): string {
    if (!b.output || !b.end) return "";
    const buf = this.term.buffer.active;
    const lines: string[] = [];
    for (let y = b.output.line; y <= b.end.line; y++) {
      const line = buf.getLine(y);
      if (!line) break;
      const text = line.translateToString(true);
      if (line.isWrapped && lines.length) lines[lines.length - 1] += text;
      else lines.push(text);
    }
    while (lines.length && lines[lines.length - 1].trim() === "") lines.pop();
    return lines.join("\n");
  }

  info(b: Block): BlockInfo {
    return { command: b.command, output: this.outputText(b), exitCode: b.exitCode, cwd: b.cwd };
  }

  lastFailed(): Block | undefined {
    for (let i = this.blocks.length - 1; i >= 0; i--) if (this.blocks[i].exitCode !== 0) return this.blocks[i];
    return undefined;
  }

  /** Command line of the block currently running ("" if none). */
  runningCommand(): string {
    return this.state === "running" ? (this.current?.command ?? "") : "";
  }

  last(): Block | undefined {
    return this.blocks[this.blocks.length - 1];
  }

  select(b: Block | undefined) {
    this.selectionDeco?.dispose();
    this.selectionDeco = undefined;
    this.selected = b;
    if (!b?.end) return;
    const height = Math.max(1, b.end.line - b.prompt.line);
    this.selectionDeco = this.term.registerDecoration({
      marker: b.prompt,
      x: 0,
      width: this.term.cols,
      height,
      layer: "bottom",
      backgroundColor: this.selectionColor(),
    });
    const buf = this.term.buffer.active;
    const viewTop = buf.viewportY;
    if (b.prompt.line < viewTop || b.prompt.line >= viewTop + this.term.rows) {
      this.term.scrollToLine(Math.max(0, b.prompt.line - 2));
    }
  }

  /** Move the selection to the previous (-1) or next (+1) block. */
  move(dir: -1 | 1) {
    if (!this.blocks.length) return;
    const i = this.selected ? this.blocks.indexOf(this.selected) : this.blocks.length;
    const next = i + dir;
    if (next < 0) return;
    if (next >= this.blocks.length) {
      this.select(undefined);
      this.term.scrollToBottom();
      return;
    }
    this.select(this.blocks[next]);
  }
}
