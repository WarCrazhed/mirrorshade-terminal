<script lang="ts">
  import { onMount } from "svelte";
  import type { Action } from "./api";

  let { actions, onClose }: { actions: Action[]; onClose: () => void } = $props();

  let query = $state("");
  let index = $state(0);
  let inputEl: HTMLInputElement;

  function norm(s: string) {
    return s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  }

  // Every query word must appear in the label (order-free, accent-insensitive).
  let filtered = $derived.by(() => {
    const words = norm(query).split(/\s+/).filter(Boolean);
    return actions.filter((a) => words.every((w) => norm(a.label).includes(w)));
  });

  $effect(() => {
    void filtered;
    index = 0;
  });

  function run(a: Action | undefined) {
    if (!a) return;
    onClose();
    a.run();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      index = Math.min(index + 1, filtered.length - 1);
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      index = Math.max(index - 1, 0);
      e.preventDefault();
    } else if (e.key === "Enter") {
      run(filtered[index]);
      e.preventDefault();
    } else if (e.key === "Escape") {
      onClose();
      e.preventDefault();
    }
  }

  onMount(() => inputEl.focus());
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onmousedown={onClose}>
  <div class="box" onmousedown={(e) => e.stopPropagation()}>
    <input bind:this={inputEl} bind:value={query} onkeydown={onKey} placeholder="Buscar acción…" spellcheck="false" />
    <ul>
      {#each filtered as a, i (a.id)}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <li class:sel={i === index} onmousemove={() => (index = i)} onmousedown={() => run(a)}>
          <span>{a.label}</span>
          {#if a.shortcut}<kbd>{a.shortcut}</kbd>{/if}
        </li>
      {:else}
        <li class="none">Sin resultados</li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: rgb(0 0 0 / 0.3);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 80px;
  }
  .box {
    width: 520px;
    max-width: calc(100vw - 40px);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.45);
    overflow: hidden;
  }
  input {
    width: 100%;
    box-sizing: border-box;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    padding: 12px 14px;
    font: 14px -apple-system, sans-serif;
    outline: none;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 6px;
    max-height: 360px;
    overflow-y: auto;
  }
  li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 7px 10px;
    border-radius: 6px;
    color: var(--text);
    font-size: 13px;
    cursor: default;
  }
  li.sel {
    background: var(--sel);
  }
  li.none {
    color: var(--muted);
  }
  kbd {
    background: none;
  }
</style>
