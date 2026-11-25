<script lang="ts">
  import { tick } from "svelte";
  import { api, extractCommand, splitCode, type AiKind, type AiStatus } from "./api";
  import type { BlockInfo } from "./blocks";

  let {
    status,
    onInsert,
    getContext,
    onClose,
    onRefreshStatus,
    onPickModel,
  }: {
    status: AiStatus | null;
    onInsert: (cmd: string) => void;
    getContext: () => { block?: BlockInfo; cwd: string };
    onClose: () => void;
    onRefreshStatus: () => void;
    onPickModel: () => void;
  } = $props();

  type Entry = {
    id: number;
    kind: AiKind;
    title: string;
    sent: string;
    redactions: number;
    local: boolean;
    model: string;
    answer: string;
    state: "streaming" | "done" | "error";
    error: string;
    requestId: number;
    command: string;
    dangers: string[];
  };

  let entries = $state<Entry[]>([]);
  let question = $state("");
  let includeBlock = $state(true);
  let confirmKey = $state<string | null>(null);
  let confirmDangers = $state<string[]>([]);
  let list: HTMLDivElement;
  let inputEl: HTMLTextAreaElement;
  let nextId = 1;

  export function focusInput() {
    inputEl?.focus();
  }

  function blockContext(b: BlockInfo): string {
    const code = b.exitCode === null ? "?" : String(b.exitCode);
    return `Comando: ${b.command}\nCódigo de salida: ${code}\nSalida:\n${b.output}`;
  }

  /** Start a request. Returns once it's queued; the answer streams in. */
  export async function ask(kind: AiKind, prompt: string, opts: { context?: string; cwd?: string; title?: string } = {}) {
    const entry: Entry = {
      id: nextId++,
      kind,
      title: opts.title ?? prompt,
      sent: "",
      redactions: 0,
      local: true,
      model: "",
      answer: "",
      state: "streaming",
      error: "",
      requestId: 0,
      command: "",
      dangers: [],
    };
    entries.push(entry);
    const e = entries[entries.length - 1]; // reactive proxy
    await tick();
    list?.scrollTo({ top: list.scrollHeight });

    e.requestId = await api.aiAsk({ kind, prompt, context: opts.context, cwd: opts.cwd }, async (ev) => {
      if (ev.type === "context") {
        e.sent = ev.sent;
        e.redactions = ev.redactions;
        e.local = ev.local;
        e.model = ev.model;
      } else if (ev.type === "delta") {
        e.answer += ev.text;
        list?.scrollTo({ top: list.scrollHeight });
      } else if (ev.type === "error") {
        e.state = "error";
        e.error = ev.message;
        onRefreshStatus();
      } else if (ev.type === "done") {
        e.state = "done";
        if (kind === "command") {
          e.command = extractCommand(e.answer);
          e.dangers = e.command ? await api.checkCommand(e.command) : [];
          await tick();
          // Enter inserts (not runs) the command.
          (document.querySelector(`[data-insert="${e.id}"]`) as HTMLButtonElement | null)?.focus();
        }
        await tick();
        list?.scrollTo({ top: list.scrollHeight });
      }
    });
  }

  export function explainBlock(b: BlockInfo, cwd: string) {
    void ask("explain", "Explica por qué falló este comando y cómo corregirlo.", {
      context: blockContext(b),
      cwd,
      title: `Explicar: ${b.command || "último comando"}`,
    });
  }

  function cancel(e: Entry) {
    if (e.state === "streaming" && e.requestId) {
      void api.aiCancel(e.requestId);
      e.state = "error";
      e.error = "Cancelado";
    }
  }

  async function tryInsert(cmd: string, key: string) {
    const dangers = await api.checkCommand(cmd);
    if (dangers.length && confirmKey !== key) {
      confirmKey = key;
      confirmDangers = dangers;
      return;
    }
    confirmKey = null;
    confirmDangers = [];
    onInsert(cmd);
  }

  function submit() {
    const q = question.trim();
    if (!q) return;
    question = "";
    const ctx = getContext();
    const context = includeBlock && ctx.block ? blockContext(ctx.block) : undefined;
    void ask("chat", q, { context, cwd: ctx.cwd });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }
</script>

<aside>
  <header>
    <div class="who">
      {#if status}
        <button class="pill" class:local={status.local} class:cloud={!status.local} class:warn={!status.ready} onclick={onPickModel} title="Cambiar modelo">
          <span class="led"></span>
          {status.provider_label} · {status.model || "modelo cargado"} ▾
        </button>
      {/if}
    </div>
    <button class="x" aria-label="Cerrar panel" onclick={onClose}>×</button>
  </header>

  {#if status && !status.ready}
    <div class="notice">
      <strong>IA no disponible.</strong>
      <code>{status.detail}</code>
      <div class="nrow">
        <button class="btn" onclick={onRefreshStatus}>Reintentar</button>
        <button class="btn" onclick={onPickModel}>Elegir otro modelo</button>
      </div>
    </div>
  {:else if status && !status.local}
    <div class="notice cloud">Modo nube: lo que envíes (ya redactado) sale a la API de Anthropic.</div>
  {/if}

  <div class="list" bind:this={list}>
    {#if entries.length === 0}
      <div class="empty">
        <p>Escribe <code># lo que quieres hacer</code> en la terminal y presiona ⏎ para generar un comando.</p>
        <p><kbd>⌘⇧E</kbd> explica el último error. <kbd>⌘↑</kbd>/<kbd>⌘↓</kbd> seleccionan bloques.</p>
        <p>Antes de enviar cualquier texto se quitan tokens, contraseñas y llaves. Abajo de cada respuesta puedes ver exactamente qué se envió.</p>
      </div>
    {/if}

    {#each entries as e (e.id)}
      <article>
        <div class="q"><span class="tag">{e.kind === "command" ? "#" : e.kind === "explain" ? "?" : "›"}</span>{e.title}</div>

        {#if e.kind === "command" && e.state === "done" && e.command}
          <div class="cmd">
            <pre>{e.command}</pre>
            {#if e.dangers.length}
              <ul class="dangers">
                {#each e.dangers as d}<li>⚠ {d}</li>{/each}
              </ul>
            {/if}
            <div class="actions">
              <button
                data-insert={e.id}
                class="btn primary"
                class:danger={e.dangers.length > 0}
                onclick={() => tryInsert(e.command, `c${e.id}`)}
              >
                {confirmKey === `c${e.id}` ? "Confirmar e insertar" : "Insertar ⏎"}
              </button>
              <button class="btn" onclick={() => navigator.clipboard.writeText(e.command)}>Copiar</button>
            </div>
            <p class="hint">Se escribe en la terminal sin ejecutarse; tú presionas ⏎.</p>
          </div>
        {:else if e.answer}
          <div class="answer">
            {#each splitCode(e.answer) as seg, i}
              {#if seg.code}
                <div class="code">
                  <pre>{seg.text}</pre>
                  {#if e.state === "done"}
                    <button class="btn" class:danger={confirmKey === `s${e.id}-${i}`} onclick={() => tryInsert(seg.text, `s${e.id}-${i}`)}>
                      {confirmKey === `s${e.id}-${i}` ? "Confirmar" : "Insertar"}
                    </button>
                  {/if}
                </div>
                {#if confirmKey === `s${e.id}-${i}`}
                  <ul class="dangers">
                    {#each confirmDangers as d}<li>⚠ {d}</li>{/each}
                  </ul>
                {/if}
              {:else}
                <p>{seg.text.trim()}</p>
              {/if}
            {/each}
          </div>
        {/if}

        {#if e.state === "streaming"}
          <div class="streaming">
            <span class="dot"></span> Pensando… <button class="link" onclick={() => cancel(e)}>Cancelar</button>
          </div>
        {:else if e.state === "error"}
          <div class="error">{e.error}</div>
        {/if}

        {#if e.sent}
          <details>
            <summary>
              {e.local ? "Enviado a modelo local" : "Enviado a Anthropic"}
              {#if e.redactions > 0}<span class="red">· {e.redactions} secreto{e.redactions === 1 ? "" : "s"} redactado{e.redactions === 1 ? "" : "s"}</span>{/if}
            </summary>
            <pre>{e.sent}</pre>
          </details>
        {/if}
      </article>
    {/each}
  </div>

  <footer>
    <textarea
      bind:this={inputEl}
      bind:value={question}
      onkeydown={onKey}
      rows="2"
      placeholder="Pregunta a la IA… (⏎ enviar)"
    ></textarea>
    <label><input type="checkbox" bind:checked={includeBlock} /> Incluir bloque seleccionado o último</label>
  </footer>
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    width: 380px;
    flex: none;
    border-left: 1px solid var(--border);
    background: var(--surface-a);
    font-size: 12.5px;
    user-select: text;
    -webkit-user-select: text;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }
  .pill {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 300px;
    padding: 3px 10px;
    border-radius: 12px;
    font-size: 11px;
    background: var(--surface-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pill:hover {
    background: var(--border);
  }
  .led {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: var(--ok);
  }
  .pill.cloud .led {
    background: var(--warn);
  }
  .pill.warn .led {
    background: var(--err);
  }
  .x {
    width: 22px;
    height: 22px;
    border-radius: 5px;
    font-size: 16px;
    color: var(--muted);
  }
  .x:hover {
    background: var(--surface-2);
  }
  .notice {
    margin: 8px 10px 0;
    padding: 8px 10px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--warn) 14%, var(--bg));
    color: var(--text);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .notice code {
    white-space: pre-wrap;
    font: 11.5px "SF Mono", Menlo, monospace;
  }
  .nrow {
    display: flex;
    gap: 6px;
  }
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .empty {
    color: var(--muted);
    line-height: 1.5;
  }
  .empty code {
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--surface-2);
    color: var(--text);
    font: 11px "SF Mono", Menlo, monospace;
  }
  article {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .q {
    color: var(--text);
    font-weight: 600;
  }
  .tag {
    display: inline-block;
    width: 16px;
    color: var(--accent);
  }
  pre {
    margin: 0;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--code-bg);
    border: 1px solid var(--border);
    color: var(--text);
    font: 12px "SF Mono", Menlo, monospace;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .cmd {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  button.link {
    color: var(--accent);
    padding: 0 4px;
  }
  .dangers {
    margin: 0;
    padding: 6px 10px;
    list-style: none;
    border-radius: 6px;
    background: color-mix(in srgb, var(--err) 16%, var(--bg));
    color: var(--err);
  }
  .hint {
    margin: 0;
    color: var(--faint);
    font-size: 11px;
  }
  .answer {
    display: flex;
    flex-direction: column;
    gap: 6px;
    color: var(--text);
    line-height: 1.5;
  }
  .answer p {
    margin: 0;
    white-space: pre-wrap;
  }
  .code {
    position: relative;
  }
  .code .btn {
    position: absolute;
    top: 4px;
    right: 4px;
    padding: 2px 8px;
    font-size: 11px;
  }
  .streaming {
    color: var(--muted);
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    animation: pulse 1s infinite alternate;
  }
  @keyframes pulse {
    to {
      opacity: 0.2;
    }
  }
  .error {
    color: var(--err);
  }
  details {
    color: var(--faint);
    font-size: 11px;
  }
  summary {
    cursor: default;
  }
  .red {
    color: var(--warn);
  }
  details pre {
    margin-top: 6px;
    max-height: 200px;
    overflow: auto;
    color: var(--muted);
    font-size: 11px;
  }
  footer {
    padding: 8px 10px 10px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  textarea {
    resize: none;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--code-bg);
    color: var(--text);
    padding: 6px 8px;
    font: 12.5px -apple-system, sans-serif;
    outline: none;
  }
  textarea:focus {
    border-color: var(--accent);
  }
  label {
    color: var(--muted);
    font-size: 11px;
    display: flex;
    gap: 6px;
    align-items: center;
  }
</style>
