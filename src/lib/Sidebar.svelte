<script lang="ts">
  import type { TabView } from "./api";

  let {
    width,
    onResize,
    tabs,
    activeId,
    onSelect,
    onClose,
    onNew,
    onSettings,
  }: {
    width: number;
    onResize: (width: number) => void;
    tabs: TabView[];
    activeId: number;
    onSelect: (id: number) => void;
    onClose: (id: number) => void;
    onNew: () => void;
    onSettings: () => void;
  } = $props();

  function shortCwd(cwd: string, home: string): string {
    if (!cwd) return "";
    const p = home && cwd.startsWith(home) ? "~" + cwd.slice(home.length) : cwd;
    const parts = p.split("/").filter(Boolean);
    return parts.length > 3 ? "…/" + parts.slice(-2).join("/") : p;
  }

  function name(t: TabView): string {
    if (t.title && t.title !== "zsh") return t.title;
    if (!t.cwd) return "zsh";
    const last = t.cwd.split("/").filter(Boolean).pop();
    return last ?? "/";
  }

  let dragging = $state(false);

  function startDrag(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    dragging = true;
    const move = (ev: PointerEvent) => onResize(Math.round(Math.min(480, Math.max(160, ev.clientX))));
    const up = () => {
      dragging = false;
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    e.preventDefault();
  }

  // Best guess at $HOME from the first absolute /Users/<name> path we see.
  let home = $derived(tabs.map((t) => t.cwd.match(/^\/Users\/[^/]+/)?.[0]).find(Boolean) ?? "");
</script>

<aside style="width:{width}px">
  <div class="top" data-tauri-drag-region></div>
  <div class="head" data-tauri-drag-region>
    <span data-tauri-drag-region>Sesiones</span>
    <button class="icon" title="Nueva pestaña (⌘T)" aria-label="Nueva pestaña" onclick={onNew}>+</button>
  </div>

  <ul>
    {#each tabs as t, i (t.id)}
      <li class:active={t.id === activeId}>
        <button class="tab" onclick={() => onSelect(t.id)} title={t.cwd}>
          <span class="dot" class:busy={!!t.running}></span>
          <span class="text">
            <span class="name">{name(t)}</span>
            <span class="sub">{t.running ? `▶ ${t.running}` : shortCwd(t.cwd, home)}</span>
          </span>
          {#if i < 9}<span class="num">⌘{i + 1}</span>{/if}
        </button>
        <button class="close" aria-label="Cerrar pestaña" title="Cerrar (⌘W)" onclick={() => onClose(t.id)}>×</button>
      </li>
    {/each}
  </ul>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="resizer"
    class:dragging
    title="Arrastra para cambiar el ancho · doble clic para restaurar"
    onpointerdown={startDrag}
    ondblclick={() => onResize(220)}
  ></div>

  <div class="bottom">
    <button class="new" onclick={onNew}><span>+</span> Nueva pestaña <kbd>⌘T</kbd></button>
    <button class="icon gear" title="Ajustes (⌘,)" aria-label="Ajustes" onclick={onSettings}>⚙</button>
  </div>
</aside>

<style>
  aside {
    position: relative;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--surface-a);
    border-right: 1px solid var(--border);
  }
  .resizer {
    position: absolute;
    top: 0;
    right: -3px;
    width: 6px;
    height: 100%;
    z-index: 20;
    cursor: col-resize;
  }
  .resizer:hover,
  .resizer.dragging {
    background: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .top {
    height: 30px;
    flex: none;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 10px 6px 14px;
    color: var(--muted);
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .icon {
    width: 22px;
    height: 22px;
    border-radius: 5px;
    color: var(--muted);
    font-size: 15px;
    line-height: 1;
  }
  .icon:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  ul {
    flex: 1;
    margin: 0;
    padding: 0 6px;
    list-style: none;
    overflow-y: auto;
  }
  li {
    position: relative;
    margin-bottom: 2px;
    border-radius: 7px;
  }
  li:hover {
    background: var(--surface-2);
  }
  li.active {
    background: var(--sel);
  }
  .tab {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 28px 7px 10px;
    text-align: left;
  }
  .dot {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: var(--faint);
  }
  li.active .dot {
    background: var(--accent);
  }
  .dot.busy {
    background: var(--warn) !important;
    animation: pulse 1s infinite alternate;
  }
  @keyframes pulse {
    to {
      opacity: 0.35;
    }
  }
  .text {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .name,
  .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    color: var(--text);
    font-size: 12.5px;
    font-weight: 500;
  }
  .sub {
    color: var(--muted);
    font: 10.5px "SF Mono", Menlo, monospace;
  }
  .num {
    color: var(--faint);
    font-size: 10px;
  }
  li:hover .num {
    visibility: hidden;
  }
  .close {
    position: absolute;
    top: 50%;
    right: 6px;
    transform: translateY(-50%);
    width: 18px;
    height: 18px;
    border-radius: 4px;
    color: var(--muted);
    font-size: 14px;
    line-height: 1;
    opacity: 0;
  }
  li:hover .close {
    opacity: 1;
  }
  .close:hover {
    background: var(--border);
    color: var(--text);
  }
  .bottom {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px;
    border-top: 1px solid var(--border);
  }
  .new {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 6px;
    color: var(--muted);
    font-size: 12px;
  }
  .new span {
    font-size: 15px;
    line-height: 1;
  }
  .new kbd {
    margin-left: auto;
  }
  .new:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .gear {
    font-size: 14px;
  }
</style>
