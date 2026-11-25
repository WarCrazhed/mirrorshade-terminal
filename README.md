# Mirrorshade Terminal

Terminal propia para macOS, estilo Warp, ligera y con **privacidad primero**.
Tauri 2 + Rust + Svelte 5 + xterm.js. Desarrollado por **WarCrazhed**.

- **Bloques por comando** con código de salida y duración; seleccionar y copiar la salida de un bloque.
- **Autocompletado** sin ejecutar programas: historial con frecencia, 26 specs propias, rutas, ramas de git,
  scripts de `package.json` y ejecutables del `PATH`. También dentro de `ssh` / `docker exec -it`.
- **IA local** con Ollama o LM Studio: genera comandos, explica errores, panel de preguntas.
  Claude API es opcional y solo existe si compilas con `--features cloud-ai`.
- Pestañas verticales, **paneles divididos**, búsqueda, links con ⌘+clic y arrastrar archivos.
- **20 temas** + editor de temas propios, opacidad, desenfoque real (0–64 px) y prompt integrado
  (Bloques tipo powerline con degradado opcional, Dos líneas, Minimal o el tuyo).
- zsh, bash y fish.

## Requisitos

- macOS, Xcode Command Line Tools
- Rust estable (rustup) — edition 2024
- Node 24+ y **pnpm 10+** (el proyecto no usa npm)
- IA local, cualquiera de las dos:
  - [Ollama](https://ollama.com): `ollama pull qwen2.5-coder:7b` (modelo por defecto)
  - [LM Studio](https://lmstudio.ai): pestaña Developer → Start Server (puerto 1234) con un modelo cargado
- Para `pnpm audit`: `cargo install cargo-audit cargo-deny --locked`

## Desarrollo

```sh
pnpm install
pnpm tauri dev          # ventana con recarga en caliente (Vite en :1420)
pnpm check              # svelte-check del frontend
pnpm test               # tests de Rust (redactor, peligro, historial, autocompletado, IA, temas, links)
pnpm audit              # pnpm audit + cargo audit + cargo deny
pnpm build:local        # «Mirrorshade Terminal.app» 100 % local (sin código de nube)
pnpm build:cloud        # .app con soporte opcional para Claude API
```

El test end-to-end contra Ollama está marcado `#[ignore]`: `cd src-tauri && cargo test -- --ignored`.

El ícono se genera desde `src-tauri/icons/source/mirrorshade-icon.svg` con `pnpm tauri icon <svg>`
(después se borran las salidas de Android, iOS y Store, que no se usan).

## Uso

| Atajo | Acción |
|---|---|
| `# texto` + ⏎ | La IA genera un comando y lo **escribe sin ejecutarlo** |
| ⌘⇧E | La IA explica el último error (o el bloque seleccionado) |
| ⌘I | Panel de IA (preguntas libres, con el bloque como contexto opcional) |
| ⌘P | Paleta de comandos (temas, prompt, abrir configuración…) |
| ⌘, | Ajustes: tema, temas propios, opacidad, desenfoque, prompt, shell, modelo de IA |
| ⌘T / ⌘1…9 / ⌘⇧[ ] | Nueva pestaña (hereda la carpeta) / ir a pestaña / anterior-siguiente |
| ⌘D / ⌘⇧D | Dividir panel a la derecha / hacia abajo |
| ⌘⌥ + flechas | Moverse entre paneles |
| ⌘W / ⌘⇧W | Cerrar panel / pestaña completa |
| ⌘B | Mostrar u ocultar la barra lateral (redimensionable; doble clic la restaura) |
| ⌘F | Buscar en la salida (⏎ siguiente, ⇧⏎ anterior, Aa mayúsculas) |
| ⌘ + clic | Abrir URL o ruta (`src/a.ts:42` abre en VS Code/Cursor en esa línea) |
| Arrastrar archivos | Pega sus rutas entre comillas |
| → | Aceptar la sugerencia fantasma |
| ⌃Espacio | Menú de sugerencias: ↑↓ elegir, ⏎/Tab aceptar, Esc cerrar |
| ⌘↑ / ⌘↓ | Seleccionar bloque anterior / siguiente (Esc deselecciona) |
| ⌘⇧C | Copiar la salida del bloque seleccionado (o del último) |
| ⌘K | Limpiar pantalla |
| ⌘+ / ⌘− / ⌘0 | Tamaño de fuente |

### Configuración y datos

| Qué | Dónde |
|---|---|
| Configuración (fuente, scrollback, IA, historial) | `~/.config/mirrorshade/config.toml` (se crea en el primer arranque) |
| Ajustes de la UI | `~/Library/Application Support/com.mirrorshade.terminal/settings.json` |
| Temas propios | `…/com.mirrorshade.terminal/themes.json` |
| Historial | `…/com.mirrorshade.terminal/history.db` (SQLite, 0600) |
| API key de Claude | Llavero de macOS |

## Privacidad y seguridad

- **Nada sale de tu Mac por defecto.** Ollama y LM Studio solo aceptan URLs loopback
  (`127.0.0.1`, `localhost`, `::1`) y no pasan por proxy. Sin telemetría.
- **La nube es opcional y explícita.** El código de Claude solo se compila con `--features cloud-ai`.
  La API key va en el Llavero y la interfaz marca en naranja cuando usas IA en la nube.
- **Redactor de secretos** (`src-tauri/src/redact.rs`): antes de cualquier envío a un LLM quita tokens,
  llaves privadas, JWT, passwords y credenciales en URLs, y cambia tu `$HOME` por `~`.
  Cada respuesta muestra **exactamente** lo que se envió.
- **La IA nunca ejecuta.** El comando se escribe en el prompt, sin caracteres de control ni secuencias
  de escape, y tú presionas ⏎. Los peligrosos (`rm -rf`, `sudo`, `curl | sh`, `dd`, force push…)
  piden doble confirmación (`src-tauri/src/danger.rs`).
- **Autocompletado y prompt sin ejecutar programas.** Las ramas se leen de `.git` (ejecutar `git` en un
  repo ajeno puede correr código vía `core.fsmonitor`) y los nombres se filtran: una rama o carpeta
  llamada `$(cmd)` no se ejecuta aunque `PROMPT_SUBST` esté activo.
- **Secuencias OSC con nonce.** La integración del shell firma cada comando con un nonce aleatorio por
  sesión que no se exporta a los hijos, así que un programa no puede inyectar comandos en el historial.
- **Historial local** con permisos 0600; no guarda comandos que empiezan con espacio ni los que
  contienen secretos.
- **WebView aislado.** La CSP solo permite IPC de Tauri; sin plugins fs/shell/http. Todo pasa por
  comandos de Rust que validan sus argumentos. Los links solo abren http/https; los ejecutables solo
  se revelan en Finder.
- **Cadena de suministro.** pnpm con `minimumReleaseAge` de 1 día y sin scripts de instalación;
  `cargo deny` solo acepta crates.io y licencias permitidas.
- La transparencia usa `macOSPrivateApi` (la app no sería aceptada en la Mac App Store; no aplica).

## Estructura

```
src/
  routes/+page.svelte       pestañas, árbol de paneles, atajos, estado de IA
  lib/Terminal.svelte       xterm.js, bloques, sugerencias, # → IA, inserción segura, links
  lib/blocks.ts             bloques a partir de OSC 133 / 7 / 6973
  lib/panes.ts              árbol de paneles divididos (funciones puras)
  lib/api.ts, pty.ts        wrappers IPC + saneamiento de comandos
  lib/themes.ts             temas integrados
  lib/Sidebar.svelte        barra lateral de pestañas
  lib/AiPanel.svelte        panel de IA (stream, «lo que se envió», confirmación de peligro)
  lib/Palette.svelte        paleta ⌘P
  lib/Settings.svelte       ajustes ⌘,
  lib/ThemeEditor.svelte    editor de temas propios
  lib/ModelPicker.svelte    selector de modelo (Ollama / LM Studio / Claude)
src-tauri/
  src/lib.rs                arranque, blur, comandos IPC
  src/pty.rs                sesiones PTY (portable-pty) y elección de shell
  src/shell_integration.rs  instala la integración de zsh/bash/fish
  shell/                    mirrorshade.zsh/.bash/.fish y los zsh* de ZDOTDIR
  src/history.rs            historial SQLite + frecencia + import de ~/.zsh_history
  src/complete.rs           autocompletado + specs/specs.json
  src/ai.rs                 Ollama, LM Studio y Claude (feature cloud-ai)
  src/redact.rs             redactor de secretos
  src/danger.rs             detector de comandos peligrosos
  src/links.rs              abrir URLs y rutas
  src/prefs.rs              ajustes validados y estado del prompt
  src/themes.rs             temas propios
  src/config.rs             config TOML
  capabilities/default.json permisos mínimos de la ventana
  deny.toml                 política de cargo deny
patches/                    parche de @xterm/addon-webgl (fondo opaco en celdas dim)
.claude/                    contexto para Claude Code: CLAUDE.md, rules/, hooks/, memory/, skills/
.agents/skills/             skills de agentes (fuente); .claude, .continue y .kiro las enlazan
skills-lock.json            versiones de las skills instaladas
```

Skills incluidas (solo las del stack): `tauri-v2`, `svelte5-best-practices`, `svelte-code-writer`,
`bash-defensive-patterns` (integración de shell), `frontend-design` y `accessibility`.

## Limitaciones conocidas

- fish tiene bloques y autocompletado pero conserva su propio prompt; no se ha probado.
- En `ssh`, `docker/podman/kubectl exec -it` o una shell anidada el autocompletado usa una heurística:
  solo historial y specs (sin rutas locales). Esos comandos se guardan como `remote:<comando>`.
- RAM en reposo ~105 MB (motor web de macOS ~60 MB + app ~30 MB). El scrollback (5000 líneas por
  panel, configurable) es lo que más crece.
- Las specs son propias (26 comandos); importar `withfig/autocomplete` requiere evaluar JS.
- Con opacidad < 1, las celdas con fondo explícito siguen opacas (límite de xterm.js).
- Sin firma ni notarización (requiere cuenta de Apple Developer).

## Autor y licencia

Desarrollado por **WarCrazhed**. Licencia MIT.
