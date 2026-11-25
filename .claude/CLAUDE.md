# Mirrorshade Terminal

Terminal propia de escritorio para macOS, estilo Warp: bloques por comando, autocompletado e IA.
**Privacidad primero:** por defecto nada sale de la Mac. Desarrollado por **WarCrazhed**.

Memoria local → `.claude/memory/MEMORY.md`. Proyecto autónomo: no consume ninguna API externa.

## Stack

- **Tauri 2** (`macos-private-api` para transparencia/blur) + **Rust edition 2024**.
- PTY con `portable-pty`; historial en SQLite (`rusqlite` bundled); fuzzy con `nucleo-matcher`;
  HTTP a LLMs con `reqwest` + rustls (sin OpenSSL).
- Frontend: **Svelte 5 (runes) + TypeScript**, SvelteKit en modo SPA (`adapter-static`, salida `build/`),
  Vite 8 en el puerto **1420**. xterm.js 6 + addons fit, search, unicode11, web-links y **webgl (parcheado)**.
- **pnpm, no npm** (decisión del usuario). Sin git todavía (`git init` pendiente).

## Comandos

```bash
pnpm install
pnpm tauri dev        # app con recarga en caliente (corre `pnpm dev` en :1420)
pnpm check            # svelte-check (lo corre el hook Stop si cambió src/)
pnpm test             # cargo test — 27 tests; e2e de Ollama: cd src-tauri && cargo test -- --ignored
pnpm audit            # pnpm audit --prod + cargo audit + cargo deny check
pnpm build:local      # «Mirrorshade Terminal.app» 100 % local (sin código de nube)
pnpm build:cloud      # igual + feature `cloud-ai` (Claude API, key en Llavero)
pnpm tauri icon src-tauri/icons/source/mirrorshade-icon.svg   # regenerar íconos (luego borrar android/ios/Store)
```

## Mapa del código

```
src/routes/+page.svelte     pestañas, árbol de paneles, atajos globales, estado de IA
src/lib/Terminal.svelte     xterm, bloques, texto fantasma, menú ⌃Espacio, `# → IA`, inserción segura, links
src/lib/blocks.ts           bloques a partir de OSC 133 / 7 / 6973 (nonce)
src/lib/panes.ts            árbol inmutable de splits (layout/split/remove/neighbor)
src/lib/api.ts | pty.ts     wrappers IPC + saneamiento/extracción de comandos
src/lib/themes.ts           20 temas integrados; UI derivada de bg/fg/accent con color-mix
src/lib/{AiPanel,Palette,Settings,ThemeEditor,ModelPicker,Sidebar}.svelte   (los 3 primeros se cargan a pedido)
src-tauri/src/lib.rs        setup, blur, registro de comandos IPC
src-tauri/src/pty.rs        sesiones PTY, resolve_shell (zsh/bash/fish), kill_all al salir
src-tauri/src/shell_integration.rs + shell/*   ZDOTDIR (zsh), --rcfile (bash), --init-command (fish)
src-tauri/src/history.rs    SQLite + frecencia + import de ~/.zsh_history
src-tauri/src/complete.rs   motor de autocompletado + specs/specs.json (26 specs propias)
src-tauri/src/ai.rs         Ollama, LM Studio (loopback) y Claude (#[cfg(feature = "cloud-ai")])
src-tauri/src/redact.rs     redactor de secretos — todo texto hacia un LLM pasa por aquí
src-tauri/src/danger.rs     detector de comandos peligrosos (doble confirmación)
src-tauri/src/links.rs      open_url (solo http/https), open_path, paths_exist
src-tauri/src/prefs.rs      settings.json validado con whitelist; prompt-state para el shell
src-tauri/src/themes.rs     temas propios en themes.json (0600)
src-tauri/src/config.rs     ~/.config/mirrorshade/config.toml
```

Rutas en runtime: config `~/.config/mirrorshade/config.toml`; datos en
`~/Library/Application Support/com.mirrorshade.terminal/` (`history.db`, `settings.json`, `themes.json`,
`shell-integration/`).

## Invariantes de seguridad (no romper)

1. **Nada sale por defecto.** Las URLs de Ollama/LM Studio solo aceptan loopback y sin proxy.
   Código de nube solo detrás de `cloud-ai`. Sin telemetría, sin CDN.
2. **Todo texto hacia un LLM pasa por `redact.rs`** y la UI muestra exactamente lo enviado.
3. **La IA nunca ejecuta.** Se inserta en el prompt (sin control chars/ESC, bracketed paste, un solo write);
   el usuario pulsa ⏎. Lo peligroso (`danger.rs`) pide doble confirmación.
4. **No ejecutar programas para autocompletar o para el prompt.** Ramas se leen de `.git`
   (git puede correr código vía `core.fsmonitor`). Nombres de rama/carpeta se filtran o escapan:
   probar con `$(touch X)` y `` `touch X` `` con `PROMPT_SUBST` activo.
5. **OSC 6973 lleva nonce** por sesión, no exportado a hijos. No aceptar comandos sin nonce.
6. **WebView aislado:** CSP estricta (`connect-src` solo IPC), sin plugins fs/shell/http/opener.
   Capacidades mínimas en `capabilities/default.json`. Toda acción del sistema = comando Rust validado.
7. **Archivos de usuario 0600** (history.db, themes.json, settings). Validar en Rust, no en el frontend.
8. **Cadena de suministro:** `minimumReleaseAge: 1440`, `onlyBuiltDependencies: []`; `cargo deny` solo crates.io.
   No añadir dependencias sin necesidad clara; correr `pnpm audit` después.

## Gotchas

- `app.security.freezePrototype: true` deja la ventana sin JS (rompe SvelteKit/xterm). Dejar en false.
- **Parche** `patches/@xterm__addon-webgl@0.19.0.patch` (`bg & 0x3FFFFFF`): sin él, celdas dim pintan
  un rect opaco con opacidad < 1. Al actualizar xterm, revisar si sigue haciendo falta y regenerar.
- Con transparencia, `theme.background` de xterm = `theme.bg + '00'` (no `#00000000`: dim/inverso/cursor
  se mezclan con ese RGB). OSC 11 responde con `theme.bg`.
- El glifo powerline `` lo dibuja WebGL (customGlyphs); con el renderer DOM saldría vacío.
- Sin emoji de ancho ambiguo en el prompt: descuadran el cursor.
- `$effect` que depende de `term` corre antes de que exista (no es reactivo): enfocar en `onMount`.
- Nombres internos: funciones de shell `__ms_*`, variables de entorno `MIRRORSHADE_*`, scripts
  `mirrorshade.{zsh,bash,fish}`, prompt `two-line`. OSC 6973 es el canal propio (con nonce).
- `src-tauri/target/` pesa ~9 GB; `cargo clean` si hace falta espacio.

## Verificación

- Rust: `pnpm test` y, si tocaste IA, el e2e con `--ignored` (requiere Ollama con `qwen2.5-coder:7b`).
- Frontend: `pnpm check`. La parte visual **no se puede verificar desde aquí** (sin permiso de captura):
  dilo explícitamente y pide al usuario que pruebe en `pnpm tauri dev`.
- Shell: probar cambios de `shell/*` en un PTY real (zsh y bash 3.2 del sistema).

## Al cerrar

Pendientes vivos → `.claude/memory/needs/pendientes.md`.
