<script lang="ts">
  import type { Theme } from "./themes";

  let {
    initial,
    allThemes,
    isNew,
    onPreview,
    onSave,
    onDelete,
    onCancel,
  }: {
    initial: Theme;
    allThemes: Theme[];
    isNew: boolean;
    /** Called on every change so the whole app previews the draft live. */
    onPreview: (t: Theme) => void;
    onSave: (t: Theme) => Promise<void>;
    onDelete: (id: string) => void;
    onCancel: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let draft = $state<Theme>(structuredClone({ ...initial, ansi: [...initial.ansi] as Theme["ansi"] }));
  let saving = $state(false);
  let error = $state("");
  let confirmDelete = $state(false);

  const NAMES = ["Negro", "Rojo", "Verde", "Amarillo", "Azul", "Magenta", "Cian", "Blanco"];

  $effect(() => {
    onPreview($state.snapshot(draft) as Theme);
  });

  function copyFrom(id: string) {
    const src = allThemes.find((t) => t.id === id);
    if (!src) return;
    draft.light = src.light;
    draft.bg = src.bg;
    draft.fg = src.fg;
    draft.accent = src.accent;
    draft.cursor = src.cursor;
    draft.selection = src.selection;
    draft.ansi = [...src.ansi] as Theme["ansi"];
  }

  async function save() {
    if (!draft.name.trim()) {
      error = "Ponle un nombre a tu tema";
      return;
    }
    saving = true;
    error = "";
    try {
      await onSave($state.snapshot(draft) as Theme);
    } catch (e) {
      error = String(e);
    }
    saving = false;
  }

  let a = $derived(draft.ansi);
</script>

<div class="editor">
  <div class="head">
    <h3>{isNew ? "Nuevo tema" : "Editar tema"}</h3>
    <span class="hint">Los cambios se ven en vivo en toda la app.</span>
  </div>

  <div class="grid2">
    <label class="field">
      <span>Nombre</span>
      <input type="text" bind:value={draft.name} maxlength="40" placeholder="Mi tema" spellcheck="false" />
    </label>
    <label class="field">
      <span>Basado en</span>
      <select onchange={(e) => copyFrom((e.target as HTMLSelectElement).value)}>
        <option value="">— copiar colores de… —</option>
        {#each allThemes as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
      </select>
    </label>
  </div>

  <!-- live sample using the draft colors -->
  <div class="sample" style="background:{draft.bg}; color:{draft.fg}">
    <div>
      <span style="background:{a[4]}; color:{a[0]}">&nbsp;~/proyecto&nbsp;</span><span class="arr" style="color:{a[4]}; background:{a[2]}"></span><span style="background:{a[2]}; color:{a[0]}">&nbsp;⎇ main&nbsp;</span><span class="arr" style="color:{a[2]}"></span>
      ls -la
    </div>
    <div><span style="color:{a[12]}">src/</span>  <span style="color:{a[10]}">build.sh</span>  <span style="color:{a[14]}">enlace</span>  notas.md  <span style="color:{a[8]}">.oculto</span></div>
    <div><span style="color:{a[1]}">error:</span> algo falló  <span style="color:{a[3]}">warning:</span> cuidado  <span style="color:{a[5]}">info</span> <span style="color:{a[6]}">ok</span></div>
    <div><span style="background:{draft.selection}">texto seleccionado</span> <span class="cur" style="background:{draft.cursor}"></span></div>
  </div>

  <h4>Interfaz</h4>
  <div class="swatches ui">
    <label><input type="color" bind:value={draft.bg} /><span>Fondo</span></label>
    <label><input type="color" bind:value={draft.fg} /><span>Texto</span></label>
    <label><input type="color" bind:value={draft.accent} /><span>Acento</span></label>
    <label><input type="color" bind:value={draft.cursor} /><span>Cursor</span></label>
    <label><input type="color" bind:value={draft.selection} /><span>Selección</span></label>
    <label class="toggle">
      <input type="checkbox" class="switch" bind:checked={draft.light} /><span>Tema claro</span>
    </label>
  </div>

  <h4>Colores de la terminal</h4>
  <div class="swatches ansi">
    {#each draft.ansi as _, i (i)}
      <label title={`${NAMES[i % 8]}${i >= 8 ? " brillante" : ""} (color ${i})`}>
        <input type="color" bind:value={draft.ansi[i]} />
        <span>{NAMES[i % 8]}{i >= 8 ? "+" : ""}</span>
      </label>
    {/each}
  </div>
  <p class="note">La fila de abajo son las versiones «brillantes». El prompt de bloques usa Azul, Verde, Rojo y Negro.</p>

  {#if error}<p class="error">{error}</p>{/if}

  <div class="actions">
    {#if !isNew}
      <button
        class="btn"
        class:danger={confirmDelete}
        onclick={() => (confirmDelete ? onDelete(draft.id) : (confirmDelete = true))}
      >
        {confirmDelete ? "Confirmar: eliminar" : "Eliminar"}
      </button>
    {/if}
    <span class="spacer"></span>
    <button class="btn" onclick={onCancel}>Cancelar</button>
    <button class="btn primary" onclick={save} disabled={saving}>{saving ? "Guardando…" : "Guardar tema"}</button>
  </div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  h3 {
    margin: 0;
    font-size: 13px;
  }
  h4 {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .hint,
  .note {
    color: var(--muted);
    font-size: 11px;
  }
  .note {
    margin: 0;
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--muted);
  }
  .field input,
  .field select {
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--code-bg);
    color: var(--text);
    font: 12.5px -apple-system, sans-serif;
    outline: none;
  }
  .field input:focus,
  .field select:focus {
    border-color: var(--accent);
  }
  .sample {
    padding: 10px 12px;
    border-radius: 8px;
    box-shadow: inset 0 0 0 1px rgb(128 128 128 / 0.3);
    font: 11.5px/1.7 "SF Mono", Menlo, monospace;
    white-space: pre;
    overflow: hidden;
  }
  .arr {
    display: inline-block;
    width: 7px;
    height: 1.7em;
    vertical-align: top;
    position: relative;
  }
  .arr::before {
    content: "";
    position: absolute;
    inset: 0;
    background: currentColor;
    clip-path: polygon(0 0, 100% 50%, 0 100%);
  }
  .cur {
    display: inline-block;
    width: 7px;
    height: 1.2em;
    vertical-align: middle;
  }
  .swatches {
    display: grid;
    gap: 8px;
  }
  .swatches.ui {
    grid-template-columns: repeat(6, 1fr);
  }
  .swatches.ansi {
    grid-template-columns: repeat(8, 1fr);
  }
  .swatches label {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    color: var(--muted);
    text-align: center;
  }
  input[type="color"] {
    width: 100%;
    height: 28px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: none;
    cursor: pointer;
  }
  input[type="color"]::-webkit-color-swatch-wrapper {
    padding: 2px;
  }
  input[type="color"]::-webkit-color-swatch {
    border: 0;
    border-radius: 4px;
  }
  .toggle {
    justify-content: center;
  }
  .switch {
    appearance: none;
    width: 34px;
    height: 20px;
    border-radius: 10px;
    background: var(--border);
    position: relative;
    transition: background 0.15s;
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
  .error {
    margin: 0;
    color: var(--err);
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }
  .spacer {
    flex: 1;
  }
</style>
