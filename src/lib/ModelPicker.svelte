<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ModelCatalog } from "./api";

  let { onChange }: { onChange: () => void } = $props();

  let catalog = $state<ModelCatalog | null>(null);
  let loading = $state(false);
  let error = $state("");

  async function load() {
    loading = true;
    error = "";
    try {
      catalog = await api.aiListModels();
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }

  async function pick(provider: string, model: string) {
    try {
      await api.aiSelectModel(provider, model);
      if (catalog) {
        catalog.provider = provider;
        catalog.model = model;
      }
      onChange();
    } catch (e) {
      error = String(e);
    }
  }

  function isSelected(provider: string, model: string) {
    if (!catalog || catalog.provider !== provider) return false;
    if (catalog.model === model) return true;
    // Ollama names may omit ":latest"; LM Studio "" = first loaded model.
    return catalog.model + ":latest" === model;
  }

  onMount(load);
</script>

<div class="picker">
  <div class="bar">
    <span class="hint">Los modelos locales no envían nada fuera de tu Mac.</span>
    <button class="btn" onclick={load} disabled={loading}>{loading ? "Buscando…" : "Actualizar"}</button>
  </div>
  {#if error}<p class="error">{error}</p>{/if}

  {#if catalog}
    {#each catalog.providers as p (p.id)}
      <section class:off={!p.available}>
        <header>
          <span class="led" class:on={p.available} class:cloud={!p.local}></span>
          <strong>{p.label}</strong>
          <span class="tag" class:cloud={!p.local}>{p.local ? "local" : "nube"}</span>
          <span class="detail">{p.detail}</span>
        </header>
        {#if p.models.length}
          <ul>
            {#each p.models as m (m)}
              <li>
                <button class:sel={isSelected(p.id, m)} onclick={() => pick(p.id, m)}>
                  <span class="radio"></span>
                  <span class="m">{m}</span>
                </button>
              </li>
            {/each}
          </ul>
        {:else if p.id === "ollama"}
          <p class="help">Instala <code>ollama</code> y descarga un modelo: <code>ollama pull qwen2.5-coder:7b</code></p>
        {:else if p.id === "lmstudio"}
          <p class="help">En LM Studio: pestaña <em>Developer</em> → <em>Start Server</em> (puerto 1234) y carga un modelo.</p>
        {/if}
      </section>
    {/each}
  {:else if loading}
    <p class="hint">Buscando modelos…</p>
  {/if}
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .hint {
    color: var(--muted);
  }
  section {
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }
  section.off {
    opacity: 0.75;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--surface-2);
  }
  .led {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--faint);
  }
  .led.on {
    background: var(--ok);
  }
  .led.on.cloud {
    background: var(--warn);
  }
  .tag {
    padding: 1px 6px;
    border-radius: 8px;
    font-size: 10px;
    background: color-mix(in srgb, var(--ok) 20%, transparent);
    color: var(--ok);
  }
  .tag.cloud {
    background: color-mix(in srgb, var(--warn) 20%, transparent);
    color: var(--warn);
  }
  .detail {
    margin-left: auto;
    color: var(--muted);
    font-size: 11px;
    text-align: right;
  }
  ul {
    margin: 0;
    padding: 4px;
    list-style: none;
    max-height: 180px;
    overflow-y: auto;
  }
  li button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 5px 8px;
    border-radius: 5px;
    text-align: left;
  }
  li button:hover {
    background: var(--surface-2);
  }
  li button.sel {
    background: var(--sel);
  }
  .radio {
    width: 10px;
    height: 10px;
    flex: none;
    border-radius: 50%;
    border: 1.5px solid var(--muted);
  }
  .sel .radio {
    border-color: var(--accent);
    background: radial-gradient(var(--accent) 45%, transparent 50%);
  }
  .m {
    font: 12px "SF Mono", Menlo, monospace;
  }
  .help {
    margin: 0;
    padding: 8px 10px;
    color: var(--muted);
    line-height: 1.5;
  }
  code {
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--code-bg);
    color: var(--text);
    font: 11px "SF Mono", Menlo, monospace;
  }
  .error {
    margin: 0;
    color: var(--err);
  }
</style>
