<script lang="ts">
  import "$lib/global.css";
  import { onMount, tick } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import Terminal from "$lib/Terminal.svelte";
  import Sidebar from "$lib/Sidebar.svelte";
  // Panels below are loaded on first use (smaller startup, less memory).
  import type AiPanelT from "$lib/AiPanel.svelte";
  import type PaletteT from "$lib/Palette.svelte";
  import type SettingsT from "$lib/Settings.svelte";
  import { api, type Action, type AiStatus, type Prefs, type TabView } from "$lib/api";
  import { applyUiTheme, getTheme, type Theme } from "$lib/themes";
  import { layout, leaf, neighbor, paneIds, remove, setRatio, split, type Divider, type PaneNode } from "$lib/panes";

  type Pane = { id: number; title: string; cwd: string; running: string; prompt: string; shell: string; startCwd: string };
  type Tab = { id: number; root: PaneNode; focus: number };

  let nextId = 1;
  let tabs = $state<Tab[]>([]);
  let panes = $state<Record<number, Pane>>({});
  let activeId = $state(0);
  const refs: Record<number, Terminal | undefined> = {};
  const tabEls: Record<number, HTMLDivElement | undefined> = {};

  let prefs = $state<Prefs>({
    theme: "tokyo-night",
    opacity: 1,
    blur_radius: 0,
    font_size: 0,
    prompt_theme: "blocks",
    prompt_gradient: false,
    sidebar: true,
    sidebar_width: 220,
    shell: "auto",
    ai_provider: "ollama",
    ai_model: "",
  });
  let baseFontSize = 13;
  let scrollback = $state(5000);
  let fontFamily = $state('"SF Mono", Menlo, Monaco, monospace');
  let configPath = "";
  let ready = $state(false);

  let fontSize = $derived(prefs.font_size || baseFontSize);
  let customThemes = $state<Theme[]>([]);
  // While the theme editor is open, the draft previews live over everything.
  let previewTheme = $state<Theme | null>(null);
  let theme = $derived(previewTheme ?? getTheme(prefs.theme, customThemes));

  let AiPanelC = $state<typeof AiPanelT | null>(null);
  let PaletteC = $state<typeof PaletteT | null>(null);
  let SettingsC = $state<typeof SettingsT | null>(null);

  let panelOpen = $state(false);
  let panel: any = $state();
  let status = $state<AiStatus | null>(null);
  let paletteOpen = $state(false);
  let settingsOpen = $state<null | "appearance" | "prompt" | "ai">(null);
  let keyModal = $state(false);
  let apiKey = $state("");
  let toast = $state("");
  let dropping = $state(false);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  let activeTab = $derived(tabs.find((t) => t.id === activeId));
  /** Focused pane of the visible tab. */
  const active = () => (activeTab ? refs[activeTab.focus] : undefined);
  let focusedPane = $derived(activeTab ? panes[activeTab.focus] : undefined);

  // What the sidebar shows for each tab: its focused pane.
  let tabViews = $derived<TabView[]>(
    tabs.map((t) => {
      const p = panes[t.focus];
      const n = paneIds(t.root).length;
      return {
        id: t.id,
        title: (p?.title ?? "zsh") + (n > 1 ? ` · ${n} paneles` : ""),
        cwd: p?.cwd ?? "",
        running: p?.running ?? "",
      };
    }),
  );

  $effect(() => applyUiTheme(theme, prefs.opacity));

  function notify(msg: string) {
    toast = msg;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 2200);
  }

  async function refreshStatus() {
    status = await api.aiStatus().catch(() => null);
  }

  // ---------- lazy panels ----------

  async function openSettings(section: "appearance" | "prompt" | "ai") {
    SettingsC ??= (await import("$lib/Settings.svelte")).default;
    settingsOpen = section;
  }

  async function togglePalette() {
    if (paletteOpen) {
      paletteOpen = false;
      return;
    }
    PaletteC ??= (await import("$lib/Palette.svelte")).default;
    paletteOpen = true;
  }

  // ---------- prefs ----------

  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  function updatePrefs(patch: Partial<Prefs>) {
    Object.assign(prefs, patch);
    clearTimeout(saveTimer);
    // Sliders fire a lot; save once they settle.
    saveTimer = setTimeout(async () => {
      try {
        const clean = await api.prefsSet($state.snapshot(prefs));
        Object.assign(prefs, clean);
        syncPrompt(); // cheap; keeps shells in sync even when patches were merged
      } catch (e) {
        notify(String(e));
      }
    }, 150);
  }

  /** Running shells re-read the prompt style/colors before each prompt. */
  function syncPrompt() {
    void api.promptSync([...getTheme(prefs.theme, customThemes).ansi]).catch((e) => notify(String(e)));
  }

  function setPrompt(prompt_theme: Prefs["prompt_theme"], prompt_gradient = prefs.prompt_gradient) {
    updatePrefs({ prompt_theme, prompt_gradient });
    notify("Prompt cambiado · se aplica al presionar Enter");
  }

  // ---------- tabs & panes ----------

  function newPane(startCwd = ""): number {
    const id = nextId++;
    panes[id] = {
      id,
      title: "zsh",
      cwd: "",
      running: "",
      prompt: prefs.prompt_theme,
      shell: prefs.shell,
      startCwd,
    };
    return id;
  }

  function newTab() {
    // Like splits: start where the focused pane currently is (~ if none yet).
    const pid = newPane(focusedPane?.cwd ?? "");
    const tab: Tab = { id: nextId++, root: leaf(pid), focus: pid };
    tabs.push(tab);
    activeId = tab.id;
  }

  function closeTab(id: number) {
    const i = tabs.findIndex((t) => t.id === id);
    if (i === -1) return;
    for (const pid of paneIds(tabs[i].root)) {
      delete panes[pid];
      delete refs[pid];
    }
    tabs.splice(i, 1);
    if (activeId === id) activeId = tabs.length ? tabs[Math.min(i, tabs.length - 1)].id : 0;
  }

  function splitPane(dir: "row" | "col") {
    const tab = activeTab;
    if (!tab) return;
    const pid = newPane(panes[tab.focus]?.cwd ?? "");
    tab.root = split(tab.root, tab.focus, dir, pid);
    tab.focus = pid;
  }

  /** Close one pane; closing the last pane closes the tab. */
  function closePane(pid: number) {
    const tab = tabs.find((t) => paneIds(t.root).includes(pid));
    if (!tab) return;
    const rest = remove(tab.root, pid);
    if (!rest) {
      closeTab(tab.id);
      return;
    }
    const neighbourId = neighbor(tab.root, pid, "left") ?? neighbor(tab.root, pid, "up") ?? paneIds(rest)[0];
    tab.root = rest;
    if (tab.focus === pid) tab.focus = paneIds(rest).includes(neighbourId) ? neighbourId : paneIds(rest)[0];
    delete panes[pid];
    delete refs[pid];
  }

  function focusDir(dir: "left" | "right" | "up" | "down") {
    const tab = activeTab;
    if (!tab) return;
    const n = neighbor(tab.root, tab.focus, dir);
    if (n !== undefined) tab.focus = n;
  }

  function updatePane(id: number, patch: Partial<Pane>) {
    const p = panes[id];
    if (p) Object.assign(p, patch);
  }

  function dragDivider(e: PointerEvent, tab: Tab, d: Divider) {
    const el = e.currentTarget as HTMLElement;
    const box = tabEls[tab.id]?.getBoundingClientRect();
    if (!box) return;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const ratio =
        d.dir === "row"
          ? (ev.clientX - box.left - d.rect.x * box.width) / (d.rect.w * box.width)
          : (ev.clientY - box.top - d.rect.y * box.height) / (d.rect.h * box.height);
      tab.root = setRatio(tab.root, d.path, ratio);
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    e.preventDefault();
  }

  // ---------- AI ----------

  async function openPanel(focusInput = false) {
    AiPanelC ??= (await import("$lib/AiPanel.svelte")).default;
    panelOpen = true;
    void refreshStatus();
    await tick();
    if (focusInput) panel?.focusInput();
  }

  function togglePanel() {
    if (panelOpen) {
      panelOpen = false;
      active()?.focus();
    } else {
      void openPanel(true);
    }
  }

  async function aiCommand(query: string, cwd: string) {
    await openPanel();
    void panel?.ask("command", query, { cwd });
  }

  async function explainLastError() {
    const t = active();
    const b = t?.lastFailedBlock();
    if (!t || !b) {
      notify("No hay ningún comando fallido reciente");
      return;
    }
    await openPanel();
    panel?.explainBlock(b, t.cwd());
  }

  async function saveKey() {
    try {
      await api.aiSetApiKey(apiKey);
      notify(apiKey.trim() ? "API key guardada en el Llavero" : "API key borrada");
    } catch (e) {
      notify(String(e));
    }
    apiKey = "";
    keyModal = false;
    void refreshStatus();
  }

  // ---------- font / config ----------

  function zoom(delta: number) {
    updatePrefs({ font_size: delta === 0 ? 0 : Math.min(28, Math.max(9, fontSize + delta)) });
  }

  async function reloadConfig() {
    const c = await api.reloadConfig();
    baseFontSize = c.font_size;
    fontFamily = c.font_family;
    configPath = c.config_path;
    scrollback = c.scrollback;
    await refreshStatus();
    notify("Configuración recargada");
  }

  async function saveTheme(t: Theme) {
    customThemes = await api.themeSave(t);
    previewTheme = null;
    updatePrefs({ theme: t.id });
    notify(`Tema «${t.name}» guardado`);
  }

  async function deleteTheme(id: string) {
    try {
      customThemes = await api.themeDelete(id);
      if (prefs.theme === id) updatePrefs({ theme: "tokyo-night" });
      notify("Tema eliminado");
    } catch (e) {
      notify(String(e));
    }
  }

  function closeSettings() {
    previewTheme = null;
    settingsOpen = null;
    active()?.focus();
  }

  // ---------- palette ----------

  let actions = $derived.by<Action[]>(() => {
    const list: Action[] = [
      { id: "new-tab", label: "Nueva pestaña", shortcut: "⌘T", run: newTab },
      { id: "split-right", label: "Dividir panel a la derecha", shortcut: "⌘D", run: () => splitPane("row") },
      { id: "split-down", label: "Dividir panel hacia abajo", shortcut: "⌘⇧D", run: () => splitPane("col") },
      { id: "close-pane", label: "Cerrar panel", shortcut: "⌘W", run: () => activeTab && closePane(activeTab.focus) },
      { id: "close-tab", label: "Cerrar pestaña completa", shortcut: "⌘⇧W", run: () => closeTab(activeId) },
      { id: "find", label: "Buscar en la salida", shortcut: "⌘F", run: () => active()?.openSearch() },
      { id: "sidebar", label: "Mostrar/ocultar barra lateral", shortcut: "⌘B", run: () => updatePrefs({ sidebar: !prefs.sidebar }) },
      { id: "clear", label: "Limpiar pantalla", shortcut: "⌘K", run: () => active()?.clear() },
      { id: "settings", label: "Ajustes", shortcut: "⌘,", run: () => openSettings("appearance") },
      { id: "theme", label: "Cambiar tema / opacidad / desenfoque", run: () => openSettings("appearance") },
      { id: "prompt", label: "Terminal: prompt (tipo oh-my-zsh) y shell", run: () => openSettings("prompt") },
      { id: "p-blocks", label: "Prompt: Bloques", run: () => setPrompt("blocks", false) },
      { id: "p-grad", label: "Prompt: Bloques con degradado", run: () => setPrompt("blocks", true) },
      { id: "p-two-line", label: "Prompt: Dos líneas", run: () => setPrompt("two-line") },
      { id: "p-min", label: "Prompt: Minimal", run: () => setPrompt("minimal") },
      { id: "p-none", label: "Prompt: el mío (.zshrc / .bashrc)", run: () => setPrompt("none") },
      { id: "model", label: "Elegir modelo de IA (Ollama, LM Studio…)", run: () => openSettings("ai") },
      { id: "ai", label: "Panel de IA", shortcut: "⌘I", run: togglePanel },
      { id: "explain", label: "IA: explicar último error", shortcut: "⌘⇧E", run: explainLastError },
      { id: "suggest", label: "Mostrar sugerencias", shortcut: "⌃Espacio", run: () => active()?.openSuggestions() },
      {
        id: "copy",
        label: "Copiar salida del bloque",
        shortcut: "⌘⇧C",
        run: () => notify(active()?.copyBlockOutput() ? "Salida copiada" : "No hay bloques todavía"),
      },
      { id: "prev", label: "Bloque anterior", shortcut: "⌘↑", run: () => active()?.moveBlock(-1) },
      { id: "next", label: "Bloque siguiente", shortcut: "⌘↓", run: () => active()?.moveBlock(1) },
      { id: "zoom-in", label: "Aumentar fuente", shortcut: "⌘+", run: () => zoom(1) },
      { id: "zoom-out", label: "Reducir fuente", shortcut: "⌘−", run: () => zoom(-1) },
      { id: "zoom-reset", label: "Tamaño de fuente normal", shortcut: "⌘0", run: () => zoom(0) },
      { id: "config", label: "Abrir archivo de configuración", run: () => active()?.runCommand(`\${EDITOR:-nano} '${configPath}'`) },
      { id: "reload", label: "Recargar configuración", run: reloadConfig },
    ];
    if (status?.cloud_available) {
      list.push({ id: "key", label: "Configurar API key de Claude", run: () => (keyModal = true) });
    }
    return list;
  });

  // ---------- keyboard ----------

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && panelOpen && (e.target as HTMLElement)?.closest?.(".ai")) {
      active()?.focus();
      return;
    }
    if (!e.metaKey || e.ctrlKey) return;
    const key = e.key.toLowerCase();
    const shift = e.shiftKey;
    const handled = () => e.preventDefault();

    // ⌘⌥ + arrows: move between panes
    if (e.altKey && key.startsWith("arrow")) {
      handled();
      focusDir(key.slice(5) as "left" | "right" | "up" | "down");
      return;
    }

    if (!e.altKey && !shift && key === "t") (handled(), newTab());
    else if (!shift && key === "d") (handled(), splitPane("row"));
    else if (shift && key === "d") (handled(), splitPane("col"));
    else if (!shift && key === "w") (handled(), activeTab && closePane(activeTab.focus));
    else if (shift && key === "w") (handled(), closeTab(activeId));
    else if (!shift && key === "f") (handled(), active()?.openSearch());
    else if (!shift && key === "b") (handled(), updatePrefs({ sidebar: !prefs.sidebar }));
    else if (!shift && key === "p") (handled(), togglePalette());
    else if (!shift && key === ",") (handled(), settingsOpen ? closeSettings() : openSettings("appearance"));
    else if (!shift && key === "k") (handled(), active()?.clear());
    else if (!shift && key === "i") (handled(), togglePanel());
    else if (shift && key === "e") (handled(), explainLastError());
    else if (shift && key === "c") (handled(), notify(active()?.copyBlockOutput() ? "Salida copiada" : "No hay bloques todavía"));
    else if (key === "arrowup") (handled(), active()?.moveBlock(-1));
    else if (key === "arrowdown") (handled(), active()?.moveBlock(1));
    else if (key === "=" || key === "+") (handled(), zoom(1));
    else if (key === "-") (handled(), zoom(-1));
    else if (key === "0") (handled(), zoom(0));
    else if (/^[1-9]$/.test(key)) {
      handled();
      const idx = key === "9" ? tabs.length - 1 : Number(key) - 1;
      if (tabs[idx]) activeId = tabs[idx].id;
    } else if (shift && ["[", "]", "{", "}"].includes(key) && tabs.length) {
      handled();
      const i = tabs.findIndex((t) => t.id === activeId);
      const dir = key === "[" || key === "{" ? -1 : 1;
      activeId = tabs[(i + dir + tabs.length) % tabs.length].id;
    }
  }

  onMount(() => {
    let unlistenDrop: (() => void) | undefined;
    (async () => {
      customThemes = await api.themesList().catch(() => []);
      const c = await api.config().catch(() => null);
      if (c) {
        baseFontSize = c.font_size;
        fontFamily = c.font_family;
        configPath = c.config_path;
        scrollback = c.scrollback;
        Object.assign(prefs, c.prefs);
      }
      ready = true;
      syncPrompt();
      newTab(); // start with one session, created with the saved prompt theme
      void refreshStatus();
      // Files dragged onto the window: paste their (quoted) paths.
      unlistenDrop = await getCurrentWebview().onDragDropEvent((ev) => {
        const p = ev.payload;
        if (p.type === "enter" || p.type === "over") dropping = true;
        else if (p.type === "leave") dropping = false;
        else if (p.type === "drop") {
          dropping = false;
          active()?.insertPaths(p.paths);
        }
      });
    })();
    return () => unlistenDrop?.();
  });
</script>

<svelte:window onkeydown={onKeydown} />

<main class:light={theme.light}>
  {#if prefs.sidebar}
    <Sidebar
      width={prefs.sidebar_width}
      onResize={(w) => updatePrefs({ sidebar_width: w })}
      tabs={tabViews}
      {activeId}
      onSelect={(id) => (activeId = id)}
      onClose={closeTab}
      onNew={newTab}
      onSettings={() => openSettings("appearance")}
    />
  {/if}

  <div class="content">
    <header data-tauri-drag-region class:inset={!prefs.sidebar}>
      <span class="title" data-tauri-drag-region>
        {#if focusedPane}
          {focusedPane.running ? `▶ ${focusedPane.running}` : focusedPane.cwd || "zsh"}
        {:else}
          Mirrorshade
        {/if}
      </span>
      <div class="right">
        <button
          class="ai-pill"
          class:cloud={status?.local === false}
          class:warn={status && !status.ready}
          onclick={togglePanel}
          title={status?.detail ?? "IA"}
        >
          <span class="led"></span>
          {status ? `${status.provider_label} · ${status.model || "modelo cargado"}` : "IA"}
        </button>
        <button class="icon" onclick={() => splitPane("row")} title="Dividir a la derecha (⌘D)" aria-label="Dividir panel">◫</button>
        <button class="icon" onclick={togglePalette} title="Paleta de comandos (⌘P)">⌘P</button>
        <button class="icon" onclick={() => openSettings("appearance")} title="Ajustes (⌘,)" aria-label="Ajustes">⚙</button>
      </div>
    </header>

    <div class="body">
      <section>
        {#if ready}
          {#each tabs as tab (tab.id)}
            {@const lay = layout(tab.root)}
            {@const multi = lay.panes.size > 1}
            <div class="tabview" class:hidden={tab.id !== activeId} bind:this={tabEls[tab.id]}>
              {#each paneIds(tab.root) as pid (pid)}
                {@const r = lay.panes.get(pid)!}
                {@const pane = panes[pid]}
                {#if pane}
                  <div
                    class="pane"
                    class:focused={multi && pid === tab.focus}
                    class:dim={multi && pid !== tab.focus}
                    style="left:{r.x * 100}%; top:{r.y * 100}%; width:{r.w * 100}%; height:{r.h * 100}%"
                  >
                    <Terminal
                      bind:this={refs[pid]}
                      visible={tab.id === activeId}
                      focused={tab.id === activeId && pid === tab.focus}
                      {fontSize}
                      {fontFamily}
                      {theme}
                      {scrollback}
                      promptTheme={pane.prompt}
                      shell={pane.shell}
                      startCwd={pane.startCwd}
                      onFocused={() => (tab.focus = pid)}
                      onTitle={(t) => updatePane(pid, { title: t })}
                      onStatus={(_, cwd, cmd) => updatePane(pid, { cwd, running: cmd })}
                      onExit={() => closePane(pid)}
                      onAiCommand={aiCommand}
                      onNotify={notify}
                    />
                  </div>
                {/if}
              {/each}
              {#each lay.dividers as d (d.path.join("."))}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="divider {d.dir}"
                  style={d.dir === "row"
                    ? `left:${(d.rect.x + d.rect.w * d.ratio) * 100}%; top:${d.rect.y * 100}%; height:${d.rect.h * 100}%`
                    : `top:${(d.rect.y + d.rect.h * d.ratio) * 100}%; left:${d.rect.x * 100}%; width:${d.rect.w * 100}%`}
                  onpointerdown={(e) => dragDivider(e, tab, d)}
                ></div>
              {/each}
            </div>
          {/each}
        {/if}

        {#if ready && tabs.length === 0}
          <div class="empty">
            <div class="logo">❯_</div>
            <h1>No hay sesiones abiertas</h1>
            <p>Abre una pestaña para iniciar una nueva sesión de terminal.</p>
            <button class="btn primary big" onclick={newTab}>Nueva pestaña <kbd>⌘T</kbd></button>
            <div class="tips">
              <span><kbd>⌘P</kbd> paleta de comandos</span>
              <span><kbd>⌘,</kbd> ajustes y temas</span>
              <span><kbd>⌘I</kbd> asistente de IA</span>
            </div>
          </div>
        {/if}

        {#if dropping}
          <div class="dropzone">Suelta para pegar la ruta en la terminal</div>
        {/if}
      </section>

      {#if panelOpen && AiPanelC}
        <div class="ai">
          <AiPanelC
            bind:this={panel}
            {status}
            onInsert={(cmd) => active()?.insertCommand(cmd)}
            getContext={() => ({ block: active()?.contextBlock(), cwd: active()?.cwd() ?? "" })}
            onClose={togglePanel}
            onRefreshStatus={refreshStatus}
            onPickModel={() => openSettings("ai")}
          />
        </div>
      {/if}
    </div>
  </div>

  {#if paletteOpen && PaletteC}
    <PaletteC
      {actions}
      onClose={() => {
        paletteOpen = false;
        active()?.focus();
      }}
    />
  {/if}

  {#if settingsOpen && SettingsC}
    <SettingsC
      {prefs}
      {fontSize}
      initialSection={settingsOpen}
      onChange={updatePrefs}
      onModelChange={refreshStatus}
      {customThemes}
      onPreviewTheme={(t) => (previewTheme = t)}
      onSaveTheme={saveTheme}
      onDeleteTheme={deleteTheme}
      onClose={closeSettings}
    />
  {/if}

  {#if keyModal}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="backdrop" onmousedown={() => (keyModal = false)}>
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <form class="modal" onmousedown={(e) => e.stopPropagation()} onsubmit={(e) => (e.preventDefault(), saveKey())}>
        <h3>API key de Claude</h3>
        <p>Se guarda en el Llavero de macOS; la página nunca la vuelve a leer. Déjala vacía para borrarla.</p>
        <!-- svelte-ignore a11y_autofocus -->
        <input type="password" bind:value={apiKey} placeholder="sk-ant-…" autofocus autocomplete="off" />
        <div class="row">
          <button type="button" class="btn" onclick={() => (keyModal = false)}>Cancelar</button>
          <button type="submit" class="btn primary">Guardar</button>
        </div>
      </form>
    </div>
  {/if}

  {#if toast}<div class="toast">{toast}</div>{/if}
</main>

<style>
  main {
    display: flex;
    height: 100vh;
    background: var(--bg-a);
  }
  .content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  header {
    height: 38px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 10px 0 16px;
    border-bottom: 1px solid var(--border);
  }
  header.inset {
    /* traffic lights when the sidebar is hidden */
    padding-left: 84px;
  }
  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font: 11.5px "SF Mono", Menlo, monospace;
  }
  .right {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: none;
  }
  .ai-pill {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 280px;
    padding: 3px 10px;
    border-radius: 12px;
    background: var(--surface-2);
    color: var(--muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ai-pill:hover {
    color: var(--text);
  }
  .led {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: var(--ok);
  }
  .ai-pill.cloud .led {
    background: var(--warn);
  }
  .ai-pill.warn .led {
    background: var(--err);
  }
  .icon {
    min-width: 26px;
    height: 24px;
    padding: 0 6px;
    border-radius: 6px;
    color: var(--muted);
    font-size: 11px;
  }
  .icon:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  section {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .ai {
    display: contents;
  }
  .tabview {
    position: absolute;
    inset: 0;
  }
  .tabview.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .pane {
    position: absolute;
  }
  .pane.dim {
    opacity: 0.82;
  }
  .pane.focused::after {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .divider {
    position: absolute;
    z-index: 15;
    background: var(--border);
  }
  .divider.row {
    width: 1px;
    margin-left: -0.5px;
    cursor: col-resize;
  }
  .divider.col {
    height: 1px;
    margin-top: -0.5px;
    cursor: row-resize;
  }
  .divider::before {
    /* wider invisible grab area */
    content: "";
    position: absolute;
    inset: -4px;
  }
  .divider:hover {
    background: var(--accent);
  }
  .dropzone {
    position: absolute;
    inset: 8px;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--accent);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    color: var(--text);
    font-size: 14px;
    pointer-events: none;
  }

  .empty {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    text-align: center;
  }
  .logo {
    font: 600 40px "SF Mono", Menlo, monospace;
    color: var(--accent);
    margin-bottom: 6px;
  }
  .empty h1 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }
  .empty p {
    margin: 0 0 8px;
    color: var(--muted);
    font-size: 13px;
  }
  .btn.big {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 16px;
    font-size: 13px;
  }
  .btn.big kbd {
    background: color-mix(in srgb, var(--bg) 25%, transparent);
    color: inherit;
  }
  .tips {
    display: flex;
    gap: 18px;
    margin-top: 18px;
    color: var(--faint);
    font-size: 11.5px;
  }

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: rgb(0 0 0 / 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .modal {
    width: 400px;
    padding: 16px 18px;
    border-radius: 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .modal h3 {
    margin: 0;
    font-size: 14px;
  }
  .modal p {
    margin: 0;
    color: var(--muted);
    line-height: 1.4;
  }
  .modal input {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--code-bg);
    color: var(--text);
    padding: 7px 9px;
    font: 13px "SF Mono", Menlo, monospace;
    outline: none;
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .toast {
    position: fixed;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 200;
    padding: 7px 14px;
    border-radius: 8px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text);
    font-size: 12px;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.3);
  }
</style>
