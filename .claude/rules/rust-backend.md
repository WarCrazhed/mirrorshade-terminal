---
paths:
  - "src-tauri/src/**/*.rs"
  - "src-tauri/Cargo.toml"
  - "src-tauri/capabilities/**"
  - "src-tauri/tauri.conf.json"
---

# Backend Rust (Tauri)

- **Comando IPC nuevo** = función `#[tauri::command]` en su módulo + registro en `generate_handler!`
  de `lib.rs` + wrapper tipado en `src/lib/api.ts`. Valida todo argumento en Rust (rutas, URLs, ids,
  colores, longitudes); el frontend no es frontera de confianza.
- **No agregues plugins** de Tauri (fs, shell, http, opener…) ni amplíes `capabilities/default.json`.
  Si hace falta tocar el sistema, crea un comando acotado.
- No relajes la CSP de `tauri.conf.json`. `freezePrototype` queda en `false`.
- Código de nube solo con `#[cfg(feature = "cloud-ai")]`. El build local no debe contenerlo.
- Cualquier texto que vaya a un LLM pasa por `redact::redact`. Endpoints locales: solo loopback
  (`127.0.0.1`, `localhost`, `::1`) y `reqwest` sin proxy.
- Abrir cosas: `open_url` solo http/https; ejecutables/`.app`/scripts solo se revelan (`open -R`).
- Archivos que escribe la app: permisos 0600, escritura atómica, tamaños y conteos acotados.
- No ejecutar `git` ni binarios del usuario para autocompletar o pintar el prompt.
- Tests: `#[cfg(test)] mod tests` en el mismo archivo. Los que necesitan red/servicios llevan `#[ignore]`.
- Dependencia nueva → justificarla, `cargo deny check` y `cargo audit` (`pnpm audit`).
