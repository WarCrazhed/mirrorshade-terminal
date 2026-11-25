import { Channel, invoke } from "@tauri-apps/api/core";

export interface PtyHandlers {
  onData: (bytes: Uint8Array) => void;
  onExit: (code: number) => void;
}

interface Spawned {
  id: number;
  nonce: string;
  integration: boolean;
}

/** Thin typed wrapper over the Rust PTY commands (src-tauri/src/pty.rs). */
export class Pty {
  private constructor(
    readonly id: number,
    /** Secret the shell hooks put in custom OSC sequences. */
    readonly nonce: string,
    /** True when zsh integration (blocks, cwd, command text) is active. */
    readonly integration: boolean,
  ) {}

  static async spawn(
    cols: number,
    rows: number,
    h: PtyHandlers,
    opts: { promptTheme?: string; shell?: string; cwd?: string } = {},
  ): Promise<Pty> {
    const onData = new Channel<ArrayBuffer | number[]>();
    onData.onmessage = (msg) => {
      // Raw bodies arrive as ArrayBuffer; fall back for the JSON form just in case.
      h.onData(msg instanceof ArrayBuffer ? new Uint8Array(msg) : Uint8Array.from(msg));
    };
    const onExit = new Channel<number>();
    onExit.onmessage = h.onExit;

    const s = await invoke<Spawned>("pty_spawn", {
      cols,
      rows,
      onData,
      onExit,
      promptTheme: opts.promptTheme ?? "none",
      shell: opts.shell ?? "auto",
      cwd: opts.cwd || null,
    });
    return new Pty(s.id, s.nonce, s.integration);
  }

  write(data: string): Promise<void> {
    return invoke("pty_write", { id: this.id, data });
  }

  resize(cols: number, rows: number): Promise<void> {
    return invoke("pty_resize", { id: this.id, cols, rows });
  }

  kill(): Promise<void> {
    return invoke("pty_kill", { id: this.id });
  }
}
