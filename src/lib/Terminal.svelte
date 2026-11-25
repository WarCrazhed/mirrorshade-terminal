<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal, type IMarker } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { Unicode11Addon } from "@xterm/addon-unicode11";
  import { SearchAddon } from "@xterm/addon-search";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import "@xterm/xterm/css/xterm.css";
  import { Pty } from "./pty";
  import { xtermTheme, type Theme } from "./themes";
  import { BlockTracker, type BlockInfo, type ShellState } from "./blocks";
  import { api, sanitizeForShell, type Suggestion } from "./api";

  let {
    visible,
    focused,
    fontSize,
    fontFamily,
    theme,
    promptTheme,
    shell,
    startCwd,
    scrollback,
    onFocused,
    onTitle,
    onStatus,
    onExit,
    onAiCommand,
    onNotify,
  }: {
    /** The tab containing this pane is shown. */
    visible: boolean;
    /** This pane has keyboard focus (visible tab + selected pane). */
    focused: boolean;
    fontSize: number;
    fontFamily: string;
    theme: Theme;
    /** Prompt style for the shell (applies when the pane is created). */
    promptTheme: string;
    shell: string;
    /** Folder to start in (split panes inherit their neighbour's). */
    startCwd: string;
    scrollback: number;
    onFocused: () => void;
    onTitle: (title: string) => void;
    onStatus: (state: ShellState, cwd: string, command: string) => void;
    onExit: () => void;
    /** User typed `# something` + Enter at the prompt. */
    onAiCommand: (query: string, cwd: string) => void;
    onNotify: (msg: string) => void;
  } = $props();

  let container = $state<HTMLDivElement>() as HTMLDivElement;
  let term: Terminal | undefined;
  let fit: FitAddon | undefined;
  let search: SearchAddon | undefined;
  let pty: Pty | undefined;
  let blocks: BlockTracker | undefined;

  // --- autocomplete state ---
  let input = $state("");
  let ghost = $state(""); // remainder shown after the cursor
  // Cursor position + xterm's exact cell metrics, so overlays line up with
  // the terminal's own glyphs (w/h = cell size, cl = glyph left offset).
  // ct/ch = glyph top offset and glyph box height inside the cell.
  let ghostPos = $state({ x: 0, y: 0, h: 0, w: 0, cl: 0, ct: 0, ch: 0 });
  let popup = $state<{ items: Suggestion[]; index: number } | null>(null);
  let reqSeq = 0;
  let scheduled = false;

  // --- search (⌘F) ---
  let searchOpen = $state(false);
  let searchQuery = $state("");
  let searchCase = $state(false);
  let searchResult = $state<{ index: number; count: number } | null>(null);
  let searchInput = $state<HTMLInputElement>();

  // --- link hover tooltip ---
  let linkTip = $state<{ x: number; y: number; text: string } | null>(null);

  // ---------- remote / nested shells (ssh, docker exec -it, bash…) ----------
  // Those shells have no Mirrorshade integration, so we can't get prompt marks.
  // Instead we remember where the cursor was when you started typing after
  // the last Enter ("anchor"): the text from there to the cursor is the
  // command being typed. Suggestions then use history + specs only.

  const NESTED = new Set(["ssh", "mosh", "et", "bash", "sh", "zsh", "fish", "dash", "ksh", "su", "nix-shell", "vagrant", "multipass", "limactl"]);
  let anchor: { marker: IMarker; x: number } | null = null;

  function remoteCommand(): string | undefined {
    if (!blocks || blocks.state !== "running" || !term || term.buffer.active.type !== "normal") return undefined;
    const cmd = blocks.runningCommand().trim();
    const words = cmd.replace(/^sudo\s+/, "").split(/\s+/);
    const first = words[0]?.split("/").pop() ?? "";
    if (NESTED.has(first)) return cmd;
    if (/^(docker|podman|nerdctl|kubectl|oc)$/.test(first) && /\s(exec|run)\s/.test(` ${cmd} `)) {
      if (/\s-\w*(it|ti)\b|\s-i\s.*-t\b|--interactive|--tty/.test(` ${cmd} `)) return cmd;
    }
    return undefined;
  }

  function remoteKey(cmd: string) {
    return "remote:" + cmd.slice(0, 80);
  }

  function readRange(line: number, x: number, endLine: number, endX?: number): string {
    const buf = term!.buffer.active;
    let s = "";
    for (let y = line; y <= endLine; y++) {
      const l = buf.getLine(y);
      if (!l) break;
      s += l.translateToString(y === endLine && endX === undefined, y === line ? x : 0, y === endLine ? endX : undefined);
    }
    return s;
  }

  function trackRemoteInput(data: string) {
    const cmd = remoteCommand();
    if (!cmd) {
      anchor = null;
      return;
    }
    if (data === "\r") {
      if (anchor && !anchor.marker.isDisposed) {
        const buf = term!.buffer.active;
        const typed = readRange(anchor.marker.line, anchor.x, buf.baseY + buf.cursorY).trim();
        // Skip yes/no answers and anything absurdly long.
        if (typed && typed.length < 500 && !/^(y|n|yes|no|s|si|sí)$/i.test(typed)) {
          void api.historyAdd(typed, remoteKey(cmd), 0);
        }
      }
      anchor?.marker.dispose();
      anchor = null;
      return;
    }
    if (/[\x03\x04\x0c]/.test(data)) {
      anchor?.marker.dispose();
      anchor = null;
      return;
    }
    if (!anchor) {
      const m = term!.registerMarker(0);
      if (m) anchor = { marker: m, x: term!.buffer.active.cursorX };
    }
  }

  /** What's being typed right now, locally (integration) or in a remote shell. */
  function currentLine(): { text: string; remote: string | undefined } | undefined {
    if (!blocks || !term) return undefined;
    const local = blocks.currentInput();
    if (local !== undefined) return { text: local, remote: undefined };
    const cmd = remoteCommand();
    if (!cmd || !anchor || anchor.marker.isDisposed) return undefined;
    const buf = term.buffer.active;
    const cursorLine = buf.baseY + buf.cursorY;
    if (cursorLine < anchor.marker.line) return undefined;
    const rest = buf.getLine(cursorLine)?.translateToString(false, buf.cursorX) ?? "";
    if (!/^\s*$/.test(rest) && !/^\s{3,}\S/.test(rest)) return undefined;
    return { text: readRange(anchor.marker.line, anchor.x, cursorLine, buf.cursorX), remote: remoteKey(cmd) };
  }

  // ---------- public API (used by the page through bind:this) ----------

  export function focus() {
    term?.focus();
  }

  export function cwd(): string {
    return blocks?.cwd ?? "";
  }

  /** Selected block, else the last failed one, else the last one. */
  export function contextBlock(preferFailed = true): BlockInfo | undefined {
    if (!blocks) return undefined;
    const b = blocks.selected ?? (preferFailed ? blocks.lastFailed() : undefined) ?? blocks.last();
    return b ? blocks.info(b) : undefined;
  }

  export function lastFailedBlock(): BlockInfo | undefined {
    if (!blocks) return undefined;
    const b = blocks.selected ?? blocks.lastFailed();
    return b ? blocks.info(b) : undefined;
  }

  export function moveBlock(dir: -1 | 1) {
    blocks?.move(dir);
  }

  /** Copy the selected (or last) block's output. Returns false if nothing to copy. */
  export function copyBlockOutput(): boolean {
    if (!blocks) return false;
    const b = blocks.selected ?? blocks.last();
    if (!b) return false;
    navigator.clipboard.writeText(blocks.outputText(b));
    return true;
  }

  export function clear() {
    blocks?.select(undefined);
    term?.clear();
  }

  /**
   * Type a command into the prompt WITHOUT executing it. Control characters
   * are stripped; multi-line text uses bracketed paste so the shell doesn't
   * run each line.
   */
  export function insertCommand(text: string) {
    if (!term || !pty) return;
    let clean = sanitizeForShell(text);
    const bracketed = term.modes.bracketedPasteMode;
    if (clean.includes("\n") && !bracketed) {
      clean = clean.split("\n").filter((l) => l.trim()).join(" && ");
    }
    // Ctrl+E (end of line), delete what was already typed, then insert —
    // all in ONE write so the pieces can't be reordered. Bracketed paste
    // makes zsh insert newlines instead of executing each line.
    const typed = currentLine()?.text ?? "";
    const body = bracketed ? `\x1b[200~${clean.replace(/\n/g, "\r")}\x1b[201~` : clean;
    pty.write("\x05" + "\x7f".repeat([...typed].length) + body);
    term.focus();
  }

  /** Files dropped on the window: insert their paths, shell-quoted. */
  export function insertPaths(paths: string[]) {
    if (!pty || !paths.length) return;
    const quoted = paths.map((p) => `'${sanitizeForShell(p).replace(/\n/g, "").replace(/'/g, `'\\''`)}'`).join(" ");
    trackRemoteInput(quoted);
    pty.write(quoted + " ");
    term?.focus();
  }

  /** Run an explicit user action (e.g. "open config") at the prompt. */
  export function runCommand(text: string) {
    if (!pty) return;
    pty.write("\x05\x15" + sanitizeForShell(text).replace(/\n/g, " ") + "\r");
    term?.focus();
  }

  export function openSuggestions() {
    void refreshPopup(true);
  }

  export function openSearch() {
    searchOpen = true;
    const sel = term?.getSelection();
    if (sel && !sel.includes("\n")) searchQuery = sel;
    requestAnimationFrame(() => {
      searchInput?.focus();
      searchInput?.select();
      if (searchQuery) find(1);
    });
  }

  // ---------- search ----------

  function searchOpts() {
    return {
      caseSensitive: searchCase,
      decorations: {
        matchBackground: theme.selection,
        matchBorder: theme.selection,
        matchOverviewRuler: theme.accent,
        activeMatchBackground: theme.ansi[3],
        activeMatchBorder: theme.ansi[3],
        activeMatchColorOverviewRuler: theme.ansi[3],
      },
    };
  }

  function find(dir: 1 | -1, incremental = false) {
    if (!search) return;
    if (!searchQuery) {
      search.clearDecorations();
      searchResult = null;
      return;
    }
    if (dir === 1) search.findNext(searchQuery, { ...searchOpts(), incremental });
    else search.findPrevious(searchQuery, searchOpts());
  }

  function closeSearch() {
    searchOpen = false;
    search?.clearDecorations();
    searchResult = null;
    term?.focus();
  }

  function onSearchKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      find(e.shiftKey ? -1 : 1);
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeSearch();
    } else if (e.key.toLowerCase() === "f" && e.metaKey) {
      e.preventDefault();
      searchInput?.select();
    }
  }

  // ---------- links (⌘+click) ----------

  // Paths: "src/a.ts", "./x", "~/y", "/abs/z", optionally ":line[:col]".
  const PATH_TOKEN = /[^\s'"`()<>\[\]{},;|]+/g;
  const linkCache = new Map<string, { start: number; text: string; path: string; line?: number; col?: number }[]>();

  function pathCandidates(text: string) {
    const out: { start: number; text: string; path: string; line?: number; col?: number }[] = [];
    for (const m of text.matchAll(PATH_TOKEN)) {
      let tok = m[0].replace(/[.:,]+$/, "");
      if (tok.length < 2 || /^[a-z][a-z0-9+.-]*:\/\//i.test(tok)) continue; // URLs: web-links addon
      const lc = tok.match(/^(.*?):(\d+)(?::(\d+))?$/);
      const path = lc ? lc[1] : tok;
      const looksLikePath = path.includes("/") || (lc !== null && /\.[A-Za-z0-9]{1,8}$/.test(path));
      if (!looksLikePath || path.length > 512) continue;
      out.push({ start: m.index!, text: tok, path, line: lc ? Number(lc[2]) : undefined, col: lc?.[3] ? Number(lc[3]) : undefined });
    }
    return out.slice(0, 32);
  }

  function showTip(ev: MouseEvent, text: string) {
    const r = container.getBoundingClientRect();
    linkTip = { x: ev.clientX - r.left + 12, y: ev.clientY - r.top + 14, text };
  }

  // ---------- autocomplete ----------

  function cellSize() {
    // xterm has no public API for this; read the renderer's own dimensions
    // (exact, device-pixel rounded) and fall back to measuring the screen.
    const dims = (term as any)?._core?._renderService?.dimensions;
    const css = dims?.css?.cell;
    if (css?.width && css?.height) {
      const dpr = dims.device?.cell?.width ? dims.device.cell.width / css.width : window.devicePixelRatio || 1;
      const ch = dims.device?.char;
      return {
        w: css.width as number,
        h: css.height as number,
        cl: (ch?.left ?? 0) / dpr,
        ct: (ch?.top ?? 0) / dpr,
        ch: ch?.height ? ch.height / dpr : (css.height as number),
      };
    }
    const screen = term?.element?.querySelector(".xterm-screen") as HTMLElement | null;
    if (!screen || !term) return { w: 8, h: 16, cl: 0, ct: 0, ch: 16 };
    const h = screen.clientHeight / term.rows;
    return { w: screen.clientWidth / term.cols, h, cl: 0, ct: 0, ch: h };
  }

  function cursorPixel() {
    if (!term) return { x: 0, y: 0, h: 0, w: 0, cl: 0, ct: 0, ch: 0 };
    const screen = term.element?.querySelector(".xterm-screen") as HTMLElement | null;
    const c = cellSize();
    const base = container.getBoundingClientRect();
    const s = screen?.getBoundingClientRect() ?? base;
    const buf = term.buffer.active;
    return {
      x: s.left - base.left + buf.cursorX * c.w,
      y: s.top - base.top + buf.cursorY * c.h,
      h: c.h,
      w: c.w,
      cl: c.cl,
      ct: c.ct,
      ch: c.ch,
    };
  }

  function atBottom() {
    const buf = term!.buffer.active;
    return buf.viewportY === buf.baseY;
  }

  function scheduleUpdate() {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      void updateSuggestions();
    });
  }

  async function updateSuggestions() {
    const cur = focused && blocks && term && atBottom() ? currentLine() : undefined;
    if (cur === undefined) {
      input = "";
      ghost = "";
      popup = null;
      return;
    }
    const next = cur.text;
    if (next === input && (ghost || popup)) {
      ghostPos = cursorPixel();
      return;
    }
    input = next;
    const seq = ++reqSeq;
    const key = cur.remote ?? blocks!.cwd;
    if (popup) void refreshPopup(false);
    if (!input.trim() || input.startsWith("#")) {
      ghost = "";
      return;
    }
    const full = await api.ghost(input, key).catch(() => null);
    if (seq !== reqSeq) return; // stale
    ghost = full && full.startsWith(input) ? full.slice(input.length) : "";
    ghostPos = cursorPixel();
  }

  async function refreshPopup(force: boolean) {
    if (!blocks) return;
    const cur = currentLine();
    if (cur === undefined) {
      popup = null;
      return;
    }
    const seq = ++reqSeq;
    const items = await api
      .complete(cur.text, cur.remote ?? blocks.cwd, cur.remote ? "" : blocks.pathEnv, !!cur.remote)
      .catch(() => []);
    if (seq !== reqSeq && !force) return;
    if (!items.length) {
      popup = null;
      return;
    }
    input = cur.text;
    popup = { items, index: 0 };
    ghost = "";
    ghostPos = cursorPixel();
  }

  function accept(s: Suggestion) {
    if (!pty) return;
    const data = "\x7f".repeat(s.replace) + sanitizeForShell(s.insert).replace(/\n/g, " ");
    trackRemoteInput(data);
    pty.write(data);
    // Keep the menu going after a subcommand/folder so you can drill down.
    if (s.insert.endsWith("/") || s.kind === "subcommand") {
      setTimeout(() => refreshPopup(true), 30);
    } else {
      popup = null;
    }
  }

  /** Returns false to stop xterm from handling the key. */
  function handleKey(e: KeyboardEvent): boolean {
    if (e.type !== "keydown") return !popup || !["ArrowUp", "ArrowDown", "Tab", "Enter", "Escape"].includes(e.key);
    const t = term!;

    if (popup) {
      const n = popup.items.length;
      if (e.key === "ArrowDown") {
        popup.index = (popup.index + 1) % n;
        e.preventDefault();
        return false;
      }
      if (e.key === "ArrowUp") {
        popup.index = (popup.index - 1 + n) % n;
        e.preventDefault();
        return false;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        accept(popup.items[popup.index]);
        e.preventDefault();
        return false;
      }
      if (e.key === "Escape") {
        popup = null;
        e.preventDefault();
        return false;
      }
    }

    if (e.key === " " && e.ctrlKey && !e.metaKey) {
      void refreshPopup(true);
      e.preventDefault();
      return false;
    }

    if (e.key === "ArrowRight" && ghost && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const data = sanitizeForShell(ghost);
      trackRemoteInput(data);
      pty?.write(data);
      ghost = "";
      e.preventDefault();
      return false;
    }

    if (e.key === "Enter" && !e.shiftKey && blocks) {
      const typed = blocks.currentInput(); // AI from `#` only at the local prompt
      if (typed !== undefined && typed.trimStart().startsWith("#")) {
        const query = typed.trimStart().replace(/^#+\s*/, "").trim();
        // Clear the line instead of sending the comment to the shell.
        pty?.write("\x05" + "\x7f".repeat([...typed].length));
        if (query) onAiCommand(query, blocks.cwd);
        e.preventDefault();
        return false;
      }
      ghost = "";
    }

    if (e.key === "Escape" && blocks?.selected) {
      blocks.select(undefined);
      return false;
    }

    // Cmd+C copies the xterm selection (it isn't a DOM selection).
    if (e.metaKey && e.key === "c" && t.hasSelection()) {
      navigator.clipboard.writeText(t.getSelection());
      t.clearSelection();
      return false;
    }
    // Other Cmd shortcuts belong to the app / native menu (paste, quit…).
    if (e.metaKey) return false;
    return true;
  }

  // ---------- lifecycle ----------

  function refit() {
    if (!term || !fit || !visible) return;
    if (!container.clientWidth || !container.clientHeight) return;
    fit.fit();
  }

  $effect(() => {
    if (visible && term) requestAnimationFrame(refit);
  });

  $effect(() => {
    if (focused && term) requestAnimationFrame(() => term?.focus());
    if (!focused) {
      ghost = "";
      popup = null;
    }
  });

  $effect(() => {
    const size = fontSize;
    const family = fontFamily;
    if (term && (term.options.fontSize !== size || term.options.fontFamily !== family)) {
      term.options.fontSize = size;
      term.options.fontFamily = family;
      requestAnimationFrame(refit);
    }
  });

  $effect(() => {
    const th = xtermTheme(theme);
    if (term) term.options.theme = th;
  });

  $effect(() => {
    if (term && scrollback > 0) term.options.scrollback = scrollback;
  });

  onMount(() => {
    const t = new Terminal({
      fontFamily,
      fontSize,
      lineHeight: 1.2,
      cursorBlink: true,
      cursorStyle: "bar",
      macOptionIsMeta: true,
      allowProposedApi: true, // unicode11 addon + decorations
      scrollback: scrollback > 0 ? scrollback : 5000,
      theme: xtermTheme(theme),
      allowTransparency: true,
    });
    term = t;
    fit = new FitAddon();
    t.loadAddon(fit);
    t.loadAddon(new Unicode11Addon());
    t.unicode.activeVersion = "11";
    search = new SearchAddon();
    t.loadAddon(search);
    search.onDidChangeResults((r) => {
      searchResult = r.resultCount > 0 ? { index: r.resultIndex + 1, count: r.resultCount } : { index: 0, count: 0 };
    });
    t.loadAddon(
      new WebLinksAddon(
        (ev, uri) => {
          if (!ev.metaKey) return;
          api.openUrl(uri).catch((e) => onNotify(String(e)));
        },
        {
          hover: (ev, text) => showTip(ev, `⌘+clic para abrir ${text.length > 60 ? text.slice(0, 57) + "…" : text}`),
          leave: () => (linkTip = null),
        },
      ),
    );
    t.registerLinkProvider({
      provideLinks(y, callback) {
        if (!blocks || remoteCommand()) return callback(undefined); // remote paths aren't on this Mac
        const text = t.buffer.active.getLine(y - 1)?.translateToString(true) ?? "";
        const cands = pathCandidates(text);
        if (!cands.length) return callback(undefined);
        const cwdNow = blocks.cwd;
        const key = cwdNow + "\n" + text;
        const make = (list: typeof cands) =>
          list.map((c) => ({
            range: { start: { x: c.start + 1, y }, end: { x: c.start + c.text.length, y } },
            text: c.text,
            decorations: { underline: true, pointerCursor: true },
            hover: (ev: MouseEvent) => showTip(ev, "⌘+clic para abrir"),
            leave: () => (linkTip = null),
            activate: (ev: MouseEvent) => {
              if (!ev.metaKey) return;
              api
                .openPath(c.path, cwdNow, c.line, c.col)
                .then((msg) => onNotify(msg))
                .catch((e) => onNotify(String(e)));
            },
          }));
        const cached = linkCache.get(key);
        if (cached) return callback(cached.length ? make(cached) : undefined);
        api
          .pathsExist(cands.map((c) => c.path), cwdNow)
          .then((exists) => {
            const found = cands.filter((_, i) => exists[i]);
            if (linkCache.size > 200) linkCache.clear();
            linkCache.set(key, found);
            callback(found.length ? make(found) : undefined);
          })
          .catch(() => callback(undefined));
      },
    });
    t.open(container);

    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose()); // falls back to DOM renderer
      t.loadAddon(webgl);
    } catch {
      // WebGL unavailable: DOM renderer is used automatically.
    }

    // Programs (vim, …) ask "what's your background?" with OSC 11 ; ? to pick
    // matching colors. Our xterm background is fully transparent (the window
    // paints it), so answer with the theme's color instead.
    t.parser.registerOscHandler(11, (data) => {
      if (data !== "?") return false; // "set background" requests: default handling
      const hex = theme.bg.replace("#", "");
      const c = (i: number) => hex.slice(i, i + 2).repeat(2);
      pty?.write(`\x1b]11;rgb:${c(0)}/${c(2)}/${c(4)}\x1b\\`);
      return true;
    });

    t.attachCustomKeyEventHandler(handleKey);
    t.onWriteParsed(scheduleUpdate);
    t.onScroll(scheduleUpdate);
    refit();
    // A new tab/pane is born focused, but the `focused` effect ran before the
    // terminal existed: take keyboard focus now so you can type right away
    // (next frame, after the click on "+" / the shortcut has finished).
    if (focused) requestAnimationFrame(() => t.focus());

    let disposed = false;
    Pty.spawn(
      t.cols,
      t.rows,
      {
        onData: (bytes) => t.write(bytes),
        onExit: () => onExit(),
      },
      { promptTheme, shell, cwd: startCwd },
    )
      .then((p) => {
        if (disposed) {
          p.kill();
          return;
        }
        pty = p;
        blocks = new BlockTracker(
          t,
          p.nonce,
          (b) => {
            if (b.command.trim()) void api.historyAdd(b.command, b.cwd, b.exitCode ?? 0);
          },
          (state, cwd, command) => {
            if (state !== "running") anchor = null;
            onStatus(state, cwd, command);
          },
          () => theme.selection,
        );
        t.onData((d) => {
          trackRemoteInput(d);
          p.write(d);
        });
        t.onResize(({ cols, rows }) => {
          p.resize(cols, rows);
          scheduleUpdate();
        });
        t.onTitleChange((title) => onTitle(title));
      })
      .catch((err) => {
        t.write(`\r\n\x1b[31mNo se pudo iniciar la shell: ${err}\x1b[0m\r\n`);
      });

    const ro = new ResizeObserver(() => refit());
    ro.observe(container);

    return () => {
      disposed = true;
      ro.disconnect();
      pty?.kill();
      t.dispose();
    };
  });

  const KIND_ICON: Record<string, string> = {
    history: "↺",
    command: "›",
    subcommand: "◆",
    option: "-",
    folder: "▸",
    file: "·",
    branch: "⎇",
    script: "▶",
  };

  let popupAbove = $derived(popup ? ghostPos.y > (container?.clientHeight ?? 0) * 0.55 : false);
  // Popup labels start exactly under the word being completed. 33px = list
  // border + padding + icon column + gap before the label.
  let tokenCells = $derived([...(input.match(/[^\s;|&]*$/)?.[0] ?? "")].length);
  let popupLeft = $derived(Math.max(0, ghostPos.x - tokenCells * ghostPos.w - 33));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="term" class:hidden={!visible} bind:this={container} onmousedown={onFocused}>
  {#if searchOpen}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="search" onmousedown={(e) => e.stopPropagation()}>
      <input
        bind:this={searchInput}
        bind:value={searchQuery}
        oninput={() => find(1, true)}
        onkeydown={onSearchKey}
        placeholder="Buscar en la salida…"
        spellcheck="false"
      />
      <span class="count">
        {#if searchResult && searchQuery}{searchResult.count ? `${searchResult.index}/${searchResult.count}` : "0"}{/if}
      </span>
      <button
        class:on={searchCase}
        title="Distinguir mayúsculas"
        onclick={() => {
          searchCase = !searchCase;
          find(1);
        }}>Aa</button
      >
      <button title="Anterior (⇧⏎)" onclick={() => find(-1)}>↑</button>
      <button title="Siguiente (⏎)" onclick={() => find(1)}>↓</button>
      <button title="Cerrar (Esc)" onclick={closeSearch}>×</button>
    </div>
  {/if}
  {#if linkTip}
    <div class="tip" style="left:{linkTip.x}px; top:{linkTip.y}px">{linkTip.text}</div>
  {/if}
  {#if ghost && focused}
    <span
      class="ghost"
      style="left:{ghostPos.x}px; top:{ghostPos.y}px; height:{ghostPos.h}px; font-size:{fontSize}px; font-family:{fontFamily}"
    >
      <!-- one box per cell, same width as xterm's cells: no drift -->
      {#each [...ghost] as ch, i (i)}<span
          class="gc"
          style="width:{ghostPos.w}px; height:{ghostPos.h}px; padding:{ghostPos.ct}px 0 0 {ghostPos.cl}px; line-height:{ghostPos.ch}px">{ch}</span
        >{/each}<kbd style="margin-top:{Math.max(0, (ghostPos.h - 16) / 2)}px">→</kbd>
    </span>
  {/if}
  {#if popup && focused}
    <ul
      class="popup"
      class:above={popupAbove}
      style="left:{popupLeft}px; font-size:{fontSize}px; font-family:{fontFamily}; {popupAbove
        ? `bottom:${(container?.clientHeight ?? 0) - ghostPos.y + 4}px`
        : `top:${ghostPos.y + ghostPos.h + 4}px`}"
    >
      {#each popup.items as item, i (i)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li
          class:sel={i === popup.index}
          role="option"
          aria-selected={i === popup.index}
          onmousedown={(e) => {
            e.preventDefault();
            accept(item);
          }}
        >
          <span class="icon k-{item.kind}">{KIND_ICON[item.kind] ?? "·"}</span>
          <span class="label">{item.label}</span>
          {#if item.description}<span class="desc">{item.description}</span>{/if}
        </li>
      {/each}
      <li class="foot">↑↓ elegir · ⏎/Tab aceptar · Esc cerrar</li>
    </ul>
  {/if}
</div>

<style>
  .term {
    position: absolute;
    inset: 0;
    padding: 8px 6px 4px 16px;
  }
  .hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .search {
    position: absolute;
    top: 8px;
    right: 14px;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px;
    border-radius: 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.3);
  }
  .search input {
    width: 200px;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    padding: 4px 6px;
    font: 12px -apple-system, sans-serif;
  }
  .search .count {
    min-width: 34px;
    color: var(--muted);
    font-size: 11px;
    text-align: right;
    padding-right: 4px;
  }
  .search button {
    min-width: 24px;
    height: 22px;
    border-radius: 5px;
    color: var(--muted);
    font-size: 12px;
  }
  .search button:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .search button.on {
    background: var(--sel);
    color: var(--text);
  }
  .tip {
    position: absolute;
    z-index: 25;
    pointer-events: none;
    padding: 3px 7px;
    border-radius: 5px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--muted);
    font: 11px -apple-system, sans-serif;
    white-space: nowrap;
  }
  .term :global(.xterm-viewport) {
    background: transparent !important;
  }

  .ghost {
    position: absolute;
    z-index: 5;
    display: flex;
    align-items: flex-start;
    pointer-events: none;
    color: var(--faint);
    white-space: pre;
  }
  .gc {
    /* Like xterm: glyph box of the font's natural height, placed char.top
       below the cell top (not CSS-centered in the cell). */
    display: inline-block;
    flex: none;
    box-sizing: border-box;
    overflow: visible;
    white-space: pre;
  }
  .ghost kbd {
    margin-left: 6px;
    padding: 0 4px;
    border: 1px solid var(--border);
    border-radius: 3px;
    font: 10px -apple-system, sans-serif;
    color: var(--muted);
    vertical-align: middle;
  }

  .popup {
    position: absolute;
    z-index: 10;
    margin: 0;
    padding: 4px;
    list-style: none;
    min-width: 260px;
    max-width: 560px;
    max-height: 280px;
    overflow-y: auto;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
    line-height: 1.5;
  }
  .popup li {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 3px 8px;
    border-radius: 5px;
    color: var(--text);
    cursor: default;
    white-space: nowrap;
  }
  .popup li.sel {
    background: var(--sel);
  }
  .icon {
    width: 12px;
    flex: none;
    text-align: center;
    color: var(--muted);
  }
  .k-history {
    color: var(--accent);
  }
  .k-subcommand,
  .k-command {
    color: var(--accent);
  }
  .k-option {
    color: var(--warn);
  }
  .k-folder,
  .k-branch {
    color: var(--ok);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .desc {
    margin-left: auto;
    padding-left: 16px;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--muted);
    font-family: -apple-system, sans-serif;
    font-size: 0.85em;
  }
  .popup li.foot {
    justify-content: center;
    margin-top: 2px;
    border-top: 1px solid var(--border);
    border-radius: 0;
    color: var(--faint);
    font: 10px -apple-system, sans-serif;
  }

  /* block decorations (elements created by xterm, so :global) */
  :global(.ms-bar) {
    width: 3px !important;
    margin-left: -10px;
    border-radius: 2px;
    cursor: pointer;
    pointer-events: auto;
    opacity: 0.5;
  }
  :global(.ms-bar:hover) {
    opacity: 1;
  }
  :global(.ms-bar.ok) {
    background: var(--ok);
  }
  :global(.ms-bar.err) {
    background: var(--err);
  }
  :global(.ms-badge) {
    font: 10px -apple-system, sans-serif;
    text-align: right;
    pointer-events: none;
    white-space: nowrap;
  }
  :global(.ms-badge.ok) {
    color: color-mix(in srgb, var(--ok) 70%, var(--bg));
  }
  :global(.ms-badge.err) {
    color: var(--err);
  }
</style>
