---
paths:
  - "src/**"
  - "patches/**"
  - "vite.config.js"
  - "svelte.config.js"
---

# Frontend (Svelte 5 + xterm.js)

- Svelte 5 **runes** (`$state`, `$derived`, `$effect`, `$props`). Nada de stores legacy ni `export let`.
- Objetos de xterm (`Terminal`, addons) no son reactivos: inicializa y enfoca en `onMount`, no en `$effect`.
- Splits: usa las funciones puras de `panes.ts`; los terminales se pintan en lista plana con clave y
  posición absoluta para que dividir **no recree** los shells.
- Escribir en el PTY lo que viene de IA o de sugerencias: sanear (sin control chars ni ESC), bracketed
  paste si es multilínea, y **un solo write** (borrar + pegar juntos). Nunca enviar `\r` por la IA.
- Alineación de overlays (fantasma, menú): medidas de `_renderService.dimensions` (css.cell, device.char)
  divididas por `devicePixelRatio`, misma fuente y tamaño que xterm.
- Colores: variables derivadas del tema (`--bg`, `--fg`, `--accent`, `--bg-a`, `--on-accent`, `--on-err`);
  texto sobre acento con `textOn()` (WCAG). Nada de colores sueltos.
- Componentes pesados (AiPanel, Palette, Settings) se importan a pedido: mantener así (RAM).
- Sin recursos remotos (fuentes, CDN, imágenes): la CSP los bloquea.
- Tras actualizar `@xterm/addon-webgl`, revisar `patches/` (ver CLAUDE.md, Gotchas).
- Verificar con `pnpm check`; la UI la prueba el usuario en `pnpm tauri dev`.
