---
paths:
  - "src-tauri/shell/**"
  - "src-tauri/src/shell_integration.rs"
  - "src-tauri/src/prefs.rs"
---

# Integración de shell (zsh / bash / fish)

- zsh: `ZDOTDIR=<AppData>/shell-integration/zsh`; `zshenv/zprofile/zshrc/zlogin.zsh` cargan los archivos
  reales del usuario y luego `mirrorshade.zsh`. bash: `--rcfile mirrorshade.bash -i` emula login. fish: `--init-command`.
- Secuencias: OSC 133 (A/B/C/D) para bloques, OSC 7 para cwd, **OSC 6973 + nonce** para el texto del
  comando. El nonce es por sesión y **no se exporta** a procesos hijos.
- El prompt relee `<AppData>/shell-integration/prompt-state` en cada precmd (lo escribe `prefs::prompt_state`
  con valores ya validados). No confíes en su contenido sin filtrar igual.
- **Nada de expansión de datos ajenos:** rama (leída de `.git`, nunca `git`), carpeta, venv y host se
  filtran o escapan (`%`, `$`, `` ` ``, `\`). En bash, PS1 con `\001/\002` y `${__ms_ps}`.
  Prueba obligatoria tras cambios: carpetas/ramas `$(touch PW)` y `` `touch PW` `` con `PROMPT_SUBST`.
- Compatibilidad: bash 3.2 de macOS (sin `declare -A`, sin `${var,,}`), zsh del sistema, fish sin probar.
- Sin emoji de ancho ambiguo; colores como índices ANSI (o truecolor en modo degradado) para seguir al tema.
- Prefijo `__ms_` para funciones/variables internas y `MIRRORSHADE_*` para el entorno (se hace `unset`
  antes de los procesos hijos). Ids de prompt: `blocks`, `two-line`, `minimal`, `none`.
- Probar en un PTY real, no solo leyendo el script.
