<script lang="ts">
  import { onMount } from "svelte";
  import type { Prefs } from "./api";
  import { THEMES, getTheme, mix, newThemeId, type Theme } from "./themes";
  import ThemeEditor from "./ThemeEditor.svelte";
  import { api } from "./api";
  import ModelPicker from "./ModelPicker.svelte";

  let {
    prefs,
    fontSize,
    initialSection = "appearance",
    onChange,
    onModelChange,
    customThemes,
    onPreviewTheme,
    onSaveTheme,
    onDeleteTheme,
    onClose,
  }: {
    prefs: Prefs;
    fontSize: number;
    initialSection?: "appearance" | "prompt" | "ai";
    onChange: (p: Partial<Prefs>) => void;
    onModelChange: () => void;
    customThemes: Theme[];
    /** Live preview while editing; null = stop previewing. */
    onPreviewTheme: (t: Theme | null) => void;
    onSaveTheme: (t: Theme) => Promise<void>;
    onDeleteTheme: (id: string) => void;
    onClose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let section = $state(initialSection);

  type Seg = { fg: string; bg?: string; bg2?: string; text: string; arrow?: boolean };

  /** Same blend the shell uses: each block fades toward the next one. */
  function withGradient(line: Seg[]): Seg[] {
    const blocks = line.filter((s) => s.bg);
    return line.map((s) => {
      if (!s.bg) return s;
      const i = blocks.indexOf(s);
      const next = blocks[i + 1]?.bg;
      return { ...s, bg2: next ? mix(s.bg, next, 0.7) : mix(s.bg, "#ffffff", 0.3) };
    });
  }

  // Previews use the selected theme's ANSI colors, like the real prompt.
  let th = $derived(getTheme(prefs.theme, customThemes));
  let allThemes = $derived([...THEMES, ...customThemes]);

  let shells = $state<string[]>([]);
  onMount(() => {
    api.shellsAvailable().then((s) => (shells = s)).catch(() => {});
  });

  // Theme being edited (null = showing the grid).
  let editing = $state<{ theme: Theme; isNew: boolean } | null>(null);

  function startNew() {
    const base = getTheme(prefs.theme, customThemes);
    editing = {
      isNew: true,
      theme: { ...base, ansi: [...base.ansi] as Theme["ansi"], id: "", name: `Mi tema ${customThemes.length + 1}`, custom: true },
    };
  }

  function startEdit(t: Theme) {
    editing = { isNew: false, theme: { ...t, ansi: [...t.ansi] as Theme["ansi"] } };
  }

  function stopEditing() {
    editing = null;
    onPreviewTheme(null);
  }

  async function saveTheme(t: Theme) {
    const theme = { ...t, id: t.id || newThemeId(t.name) };
    await onSaveTheme(theme);
    editing = null;
  }
  let PROMPTS = $derived.by(() => {
    const a = th.ansi;
    const dim = a[8];
    return [
      {
        id: "blocks",
        name: "Bloques",
        desc: "Estilo powerline/agnoster: segmentos de color con flechas. Sin instalar fuentes.",
        preview: [
          (prefs.prompt_gradient ? withGradient : (l: Seg[]) => l)([
            { bg: a[0], fg: a[1], text: " ✘ 1 " },
            { bg: a[4], fg: a[0], text: " ~/proyecto ", arrow: true },
            { bg: a[2], fg: a[0], text: " ⎇ main ", arrow: true },
            { bg: a[8], fg: a[15], text: " ⏱ 3.1s ", arrow: true },
            { fg: th.fg, text: " ", arrow: true },
          ]),
        ] as Seg[][],
      },
      {
        id: "two-line",
        name: "Dos líneas",
        desc: "Dos líneas: usuario, carpeta, rama de git, error y duración.",
        preview: [
          [
            { fg: dim, text: "╭─ " },
            { fg: a[6], text: "tú" },
            { fg: dim, text: "@mac " },
            { fg: a[4], text: "~/proyecto" },
            { fg: dim, text: " on " },
            { fg: a[5], text: "⎇ main" },
            { fg: dim, text: " ⏱ 3.1s" },
          ],
          [
            { fg: dim, text: "╰─" },
            { fg: a[2], text: "❯ " },
          ],
        ] as Seg[][],
      },
      {
        id: "minimal",
        name: "Minimal",
        desc: "Una línea: carpeta, rama y flecha que se pone roja si falla.",
        preview: [
          [
            { fg: a[4], text: "~/proyecto" },
            { fg: dim, text: " on " },
            { fg: a[5], text: "⎇ main" },
            { fg: a[2], text: " ❯ " },
          ],
        ] as Seg[][],
      },
      {
        id: "none",
        name: "El mío",
        desc: "Respeta tu prompt de .zshrc (oh-my-zsh, starship, p10k…).",
        preview: [[{ fg: th.fg, text: "tú@mac proyecto % " }]] as Seg[][],
      },
    ] as const;
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onmousedown={onClose}>
  <div class="win" onmousedown={(e) => e.stopPropagation()}>
    <nav>
      <h2>Ajustes</h2>
      <button class:sel={section === "appearance"} onclick={() => (section = "appearance")}>🎨 Apariencia</button>
      <button class:sel={section === "prompt"} onclick={() => (section = "prompt")}>❯ Terminal</button>
      <button class:sel={section === "ai"} onclick={() => (section = "ai")}>✦ Modelo de IA</button>
    </nav>

    <div class="body">
      <button class="x" aria-label="Cerrar" onclick={onClose}>×</button>

      {#if section === "appearance" && editing}
        <ThemeEditor
          initial={editing.theme}
          isNew={editing.isNew}
          {allThemes}
          onPreview={onPreviewTheme}
          onSave={saveTheme}
          onDelete={(id) => {
            onDeleteTheme(id);
            stopEditing();
          }}
          onCancel={stopEditing}
        />
      {:else if section === "appearance"}
        <h3>Tema</h3>
        <div class="themes">
          {#each allThemes as th (th.id)}
            <div class="card">
              <button
                class="theme"
                class:sel={prefs.theme === th.id}
                style="background:{th.bg}; color:{th.fg}"
                onclick={() => onChange({ theme: th.id })}
              >
                <span class="sample"><span style="color:{th.ansi[2]}">❯</span> ls <span style="color:{th.ansi[4]}">src/</span></span>
                <span class="dots">
                  {#each [1, 2, 3, 4, 5, 6] as i}<span style="background:{th.ansi[i]}"></span>{/each}
                </span>
                <span class="tname">{th.name}{#if th.custom}<em>Tuyo</em>{/if}</span>
              </button>
              {#if th.custom}
                <button class="edit" title="Editar tema" aria-label="Editar tema" onclick={() => startEdit(th)}>✎</button>
              {/if}
            </div>
          {/each}
          <button class="theme create" onclick={startNew}>
            <span class="plus">＋</span>
            <span class="tname">Crear tema</span>
          </button>
        </div>

        <h3>Ventana</h3>
        <label class="row">
          <span class="lbl">Opacidad <b>{Math.round(prefs.opacity * 100)}%</b></span>
          <input
            type="range"
            min="0.4"
            max="1"
            step="0.02"
            value={prefs.opacity}
            oninput={(e) => onChange({ opacity: Number((e.target as HTMLInputElement).value) })}
          />
        </label>
        <label class="row">
          <span class="lbl">
            Desenfoque del fondo <b>{prefs.blur_radius === 0 ? "apagado" : prefs.blur_radius}</b>
            <small>
              {#if prefs.opacity >= 1}
                Baja la opacidad para ver lo que hay detrás desenfocado.
              {:else}
                Qué tan borroso se ve lo que hay detrás de la ventana.
              {/if}
            </small>
          </span>
          <input
            type="range"
            min="0"
            max="64"
            step="1"
            value={prefs.blur_radius}
            oninput={(e) => onChange({ blur_radius: Number((e.target as HTMLInputElement).value) })}
          />
        </label>
        <label class="row">
          <span class="lbl">Tamaño de letra <b>{fontSize}px</b></span>
          <input
            type="range"
            min="9"
            max="24"
            step="1"
            value={fontSize}
            oninput={(e) => onChange({ font_size: Number((e.target as HTMLInputElement).value) })}
          />
        </label>
        <label class="row">
          <span class="lbl">Barra lateral de pestañas <small>⌘B</small></span>
          <input type="checkbox" class="switch" checked={prefs.sidebar} onchange={(e) => onChange({ sidebar: (e.target as HTMLInputElement).checked })} />
        </label>
      {:else if section === "prompt"}
        <h3>Shell</h3>
        <div class="shells">
          {#each [["auto", "Automática", "La de tu usuario ($SHELL)"], ["zsh", "zsh", ""], ["bash", "bash", ""], ["fish", "fish", ""]] as [id, label, hint] (id)}
            {@const ok = id === "auto" || shells.includes(id)}
            <button
              class="shell"
              class:sel={prefs.shell === id}
              disabled={!ok}
              title={ok ? hint : `${label} no está instalado`}
              onclick={() => onChange({ shell: id as typeof prefs.shell })}
            >
              <strong>{label}</strong>
              <small>{ok ? hint || "Bloques, autocompletado y prompt" : "No instalado"}</small>
            </button>
          {/each}
        </div>
        <p class="live">Se usa en pestañas y paneles nuevos. Con fish se conserva su propio prompt.</p>

        <h3>Estilo del prompt</h3>
        <p class="note">Como un tema de oh-my-zsh, pero integrado y sin instalar nada. La rama de git se lee sin ejecutar <code>git</code>.</p>
        <div class="prompts">
          {#each PROMPTS as p (p.id)}
            <button
              class="prompt"
              class:sel={prefs.prompt_theme === p.id}
              onclick={() => onChange({ prompt_theme: p.id })}
            >
              <div class="term" style="background:{th.bg}">
                {#each p.preview as line, li (li)}
                  <div class="pline">
                    {#each line as seg, si (si)}
                      {#if seg.arrow}
                        <!-- powerline arrow: previous segment color pointing into this one -->
                        <span
                          class="arrow"
                          style="color:{line[si - 1]?.bg2 ?? line[si - 1]?.bg ?? 'transparent'}; background:{seg.bg ?? 'transparent'}"
                        ></span>
                      {/if}
                      <span
                        style="color:{seg.fg}; background:{seg.bg2
                          ? `linear-gradient(90deg, ${seg.bg}, ${seg.bg2})`
                          : (seg.bg ?? 'transparent')}">{seg.text}</span
                      >
                    {/each}
                  </div>
                {/each}
              </div>
              <div class="pinfo">
                <strong>{p.name}</strong>
                <span>{p.desc}</span>
              </div>
            </button>
          {/each}
        </div>
        <label class="row" class:disabled={prefs.prompt_theme !== "blocks"}>
          <span class="lbl">
            Degradado en los bloques
            <small>Cada bloque se funde con el siguiente, con los colores del tema.</small>
          </span>
          <input
            type="checkbox"
            class="switch"
            disabled={prefs.prompt_theme !== "blocks"}
            checked={prefs.prompt_gradient}
            onchange={(e) => onChange({ prompt_gradient: (e.target as HTMLInputElement).checked })}
          />
        </label>
        <p class="live">Los cambios se aplican en todas las pestañas al presionar Enter.</p>
      {:else}
        <h3>Modelo de IA</h3>
        <ModelPicker onChange={onModelChange} />
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: rgb(0 0 0 / 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .win {
    width: 720px;
    max-width: calc(100vw - 40px);
    height: 520px;
    max-height: calc(100vh - 60px);
    display: flex;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.5);
    overflow: hidden;
  }
  nav {
    width: 180px;
    flex: none;
    padding: 16px 10px;
    background: var(--surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  nav h2 {
    margin: 0 8px 12px;
    font-size: 14px;
  }
  nav button {
    padding: 7px 10px;
    border-radius: 6px;
    text-align: left;
    color: var(--muted);
    font-size: 12.5px;
  }
  nav button:hover {
    background: var(--surface-2);
  }
  nav button.sel {
    background: var(--sel);
    color: var(--text);
  }
  .body {
    position: relative;
    flex: 1;
    padding: 18px 22px;
    overflow-y: auto;
  }
  .x {
    position: absolute;
    top: 10px;
    right: 10px;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    font-size: 17px;
    color: var(--muted);
  }
  .x:hover {
    background: var(--surface-2);
  }
  h3 {
    margin: 4px 0 10px;
    font-size: 12px;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  h3:not(:first-of-type) {
    margin-top: 22px;
  }
  .themes {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .theme {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border-radius: 8px;
    border: 2px solid transparent;
    box-shadow: inset 0 0 0 1px rgb(128 128 128 / 0.25);
    text-align: left;
  }
  .theme.sel {
    border-color: var(--accent);
  }
  .card {
    position: relative;
    display: flex;
  }
  .card .theme {
    flex: 1;
  }
  .edit {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    background: rgb(0 0 0 / 0.35);
    color: #fff;
    font-size: 12px;
    opacity: 0;
  }
  .card:hover .edit {
    opacity: 1;
  }
  .tname em {
    margin-left: 6px;
    padding: 0 5px;
    border-radius: 6px;
    font-style: normal;
    font-size: 9.5px;
    font-weight: 600;
    background: rgb(128 128 128 / 0.3);
  }
  .theme.create {
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--border);
    box-shadow: none;
    color: var(--muted);
  }
  .theme.create:hover {
    border-color: var(--accent);
    color: var(--text);
  }
  .plus {
    font-size: 20px;
  }
  .sample {
    font: 11px "SF Mono", Menlo, monospace;
  }
  .dots {
    display: flex;
    gap: 4px;
  }
  .dots span {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .tname {
    font-size: 11.5px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }
  .lbl {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .lbl small {
    color: var(--muted);
    font-size: 11px;
  }
  input[type="range"] {
    width: 200px;
    accent-color: var(--accent);
  }
  .switch {
    appearance: none;
    width: 34px;
    height: 20px;
    border-radius: 10px;
    background: var(--border);
    position: relative;
    transition: background 0.15s;
    flex: none;
  }
  .switch::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    transition: left 0.15s;
  }
  .switch:checked {
    background: var(--accent);
  }
  .switch:checked::after {
    left: 16px;
  }
  .note {
    margin: 0 0 12px;
    color: var(--muted);
    line-height: 1.5;
  }
  code {
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--code-bg);
    font: 11px "SF Mono", Menlo, monospace;
  }
  .prompts {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .prompt {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px;
    border-radius: 8px;
    border: 2px solid var(--border);
    text-align: left;
  }
  .prompt.sel {
    border-color: var(--accent);
  }
  .term {
    width: 300px;
    flex: none;
    padding: 8px 10px;
    border-radius: 6px;
    box-shadow: inset 0 0 0 1px rgb(128 128 128 / 0.25);
    font: 11px/1.6 "SF Mono", Menlo, monospace;
    white-space: pre;
    overflow: hidden;
  }
  .pline {
    display: flex;
    align-items: stretch;
  }
  .arrow {
    width: 8px;
    flex: none;
    position: relative;
  }
  .arrow::before {
    content: "";
    position: absolute;
    inset: 0;
    background: currentColor;
    clip-path: polygon(0 0, 100% 50%, 0 100%);
  }
  .pinfo {
    display: flex;
    flex-direction: column;
    gap: 3px;
    color: var(--muted);
    line-height: 1.4;
  }
  .pinfo strong {
    color: var(--text);
    font-size: 13px;
  }
  .shells {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }
  .shell {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border-radius: 8px;
    border: 2px solid var(--border);
    text-align: left;
  }
  .shell.sel {
    border-color: var(--accent);
  }
  .shell:disabled {
    opacity: 0.4;
  }
  .shell small {
    color: var(--muted);
    font-size: 10.5px;
  }
  .row.disabled {
    opacity: 0.45;
  }
  .live {
    margin: 10px 0 0;
    color: var(--muted);
    font-size: 11.5px;
  }
</style>
