//! AI requests. All network traffic happens here in Rust — the WebView's CSP
//! forbids it from connecting anywhere. Every prompt and context is passed
//! through the secret redactor first, and the exact redacted text is echoed
//! back to the UI so the user can see what left the terminal.
//!
//! Providers:
//! - `ollama` (default): local, native API. URL must be loopback.
//! - `lmstudio`: local, OpenAI-compatible server (`/v1/models`,
//!   `/v1/chat/completions`). URL must be loopback.
//! - `claude`: Anthropic API; compiled only with `--features cloud-ai`, API key
//!   kept in the macOS Keychain.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::ipc::Channel;
use tauri::State;

use crate::config::AiConfig;
use crate::redact;
use crate::AppState;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Natural language -> one shell command.
    Command,
    /// Explain a failed command and suggest a fix.
    Explain,
    /// Free question, optionally about a block.
    Chat,
}

#[derive(Debug, Deserialize)]
pub struct AiRequest {
    kind: Kind,
    prompt: String,
    #[serde(default)]
    context: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AiEvent {
    /// What is being sent, after redaction (shown to the user).
    Context { sent: String, redactions: usize, provider: String, model: String, local: bool },
    Delta { text: String },
    Done,
    Error { message: String },
}

#[derive(Debug, Serialize)]
pub struct AiStatus {
    provider: String,
    provider_label: String,
    model: String,
    local: bool,
    ready: bool,
    detail: String,
    cloud_available: bool,
}

#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    id: &'static str,
    label: &'static str,
    local: bool,
    available: bool,
    detail: String,
    models: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ModelCatalog {
    providers: Vec<ProviderInfo>,
    provider: String,
    model: String,
}

const SYSTEM_COMMAND: &str = "Eres un generador de comandos de shell para macOS con zsh. \
Responde ÚNICAMENTE con el comando, sin explicación, sin markdown y sin comillas invertidas. \
Prefiere un solo comando en una línea. Para el directorio actual usa rutas relativas (.), no la ruta absoluta. Si falta un dato, usa un marcador como <archivo>. \
Nunca agregues comandos destructivos que el usuario no pidió.";

const SYSTEM_EXPLAIN: &str = "Eres un asistente experto en terminal (macOS, zsh). El usuario ejecutó un comando que falló. \
Explica la causa probable en 2 a 4 frases en español y propone la corrección. \
Si propones un comando, ponlo solo en un bloque ```sh```. Sé breve.";

const SYSTEM_CHAT: &str = "Eres un asistente de terminal para macOS con zsh. Responde en español, de forma breve y precisa. \
Pon los comandos en bloques ```sh```.";

/// Claude models offered in the picker (current generation).
#[cfg(feature = "cloud-ai")]
const CLAUDE_MODELS: &[&str] = &["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"];

pub fn is_loopback_url(url: &str) -> bool {
    match reqwest::Url::parse(url) {
        Ok(u) => {
            matches!(u.scheme(), "http" | "https")
                && matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]" | "::1"))
        }
        Err(_) => false,
    }
}

fn provider_label(p: &str) -> &'static str {
    match p {
        "lmstudio" => "LM Studio",
        "claude" => "Claude",
        _ => "Ollama",
    }
}

fn build_messages(req: &AiRequest, max_context: usize) -> (&'static str, String, usize) {
    let system = match req.kind {
        Kind::Command => SYSTEM_COMMAND,
        Kind::Explain => SYSTEM_EXPLAIN,
        Kind::Chat => SYSTEM_CHAT,
    };
    let mut user = String::new();
    if let Some(cwd) = &req.cwd {
        user.push_str(&format!("Directorio actual: {cwd}\n"));
    }
    if let Some(ctx) = req.context.as_deref().filter(|c| !c.trim().is_empty()) {
        // Keep the tail: errors are usually at the end of the output.
        let chars: Vec<char> = ctx.chars().collect();
        let start = chars.len().saturating_sub(max_context);
        let tail: String = chars[start..].iter().collect();
        user.push_str("<salida_terminal>\n");
        if start > 0 {
            user.push_str("[…recortado…]\n");
        }
        user.push_str(&tail);
        user.push_str("\n</salida_terminal>\n");
    }
    user.push_str(&req.prompt);
    let r = redact::redact(&user);
    (system, r.text, r.count)
}

fn local_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy() // never route local traffic through a proxy
        .connect_timeout(Duration::from_secs(3))
        .build()
        .expect("http client")
}

fn base(url: &str) -> &str {
    url.trim_end_matches('/')
}

/// Read a streaming body line by line.
async fn for_each_line(
    resp: reqwest::Response,
    mut f: impl FnMut(&str) -> Result<bool, String>,
) -> Result<(), String> {
    let mut stream = resp.bytes_stream();
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            if f(line.trim_end())? {
                return Ok(());
            }
        }
    }
    if !buf.is_empty() {
        f(String::from_utf8_lossy(&buf).trim_end())?;
    }
    Ok(())
}

async fn check_ok(resp: reqwest::Response, who: &str) -> Result<reqwest::Response, String> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    Err(format!("{who} respondió {status}: {}", text.chars().take(300).collect::<String>()))
}

// ---------- Ollama ----------

async fn ollama_models(url: &str) -> Result<Vec<String>, String> {
    if !is_loopback_url(url) {
        return Err("ollama_url debe ser 127.0.0.1, localhost o ::1".into());
    }
    let resp = local_client()
        .get(format!("{}/api/tags", base(url)))
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map_err(|_| "Ollama no responde. Inícialo con: ollama serve (o abre la app Ollama)".to_string())?;
    let v: serde_json::Value = resp.json().await.unwrap_or_default();
    Ok(v["models"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect())
        .unwrap_or_default())
}

async fn ollama_stream(url: &str, model: &str, system: &str, user: &str, mut on_delta: impl FnMut(&str)) -> Result<(), String> {
    if !is_loopback_url(url) {
        return Err("ollama_url debe ser una dirección local (loopback)".into());
    }
    let body = serde_json::json!({
        "model": model,
        "stream": true,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "options": {"temperature": 0.2},
    });
    let resp = local_client()
        .post(format!("{}/api/chat", base(url)))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("No se pudo conectar con Ollama: {e}"))?;
    let resp = check_ok(resp, "Ollama").await?;
    for_each_line(resp, |line| {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { return Ok(false) };
        if let Some(err) = v["error"].as_str() {
            return Err(err.to_string());
        }
        if let Some(t) = v["message"]["content"].as_str().filter(|t| !t.is_empty()) {
            on_delta(t);
        }
        Ok(v["done"].as_bool() == Some(true))
    })
    .await
}

// ---------- LM Studio (OpenAI-compatible local server) ----------

async fn lmstudio_models(url: &str) -> Result<Vec<String>, String> {
    if !is_loopback_url(url) {
        return Err("lmstudio_url debe ser 127.0.0.1, localhost o ::1".into());
    }
    let resp = local_client()
        .get(format!("{}/v1/models", base(url)))
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map_err(|_| "LM Studio no responde. Abre LM Studio → Developer → Start Server (puerto 1234)".to_string())?;
    let v: serde_json::Value = resp.json().await.unwrap_or_default();
    Ok(v["data"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect())
        .unwrap_or_default())
}

async fn lmstudio_stream(url: &str, model: &str, system: &str, user: &str, mut on_delta: impl FnMut(&str)) -> Result<(), String> {
    if !is_loopback_url(url) {
        return Err("lmstudio_url debe ser una dirección local (loopback)".into());
    }
    let body = serde_json::json!({
        "model": model,
        "stream": true,
        "temperature": 0.2,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
    });
    let resp = local_client()
        .post(format!("{}/v1/chat/completions", base(url)))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("No se pudo conectar con LM Studio: {e}"))?;
    let resp = check_ok(resp, "LM Studio").await?;
    for_each_line(resp, |line| {
        let Some(data) = line.strip_prefix("data:") else { return Ok(false) };
        let data = data.trim();
        if data == "[DONE]" {
            return Ok(true);
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else { return Ok(false) };
        if let Some(err) = v["error"]["message"].as_str().or(v["error"].as_str()) {
            return Err(err.to_string());
        }
        if let Some(t) = v["choices"][0]["delta"]["content"].as_str().filter(|t| !t.is_empty()) {
            on_delta(t);
        }
        Ok(false)
    })
    .await
}

// ---------- Claude (optional) ----------

#[cfg(feature = "cloud-ai")]
pub mod claude {
    use super::*;

    const SERVICE: &str = "com.mirrorshade.terminal";
    const ACCOUNT: &str = "anthropic_api_key";

    pub fn api_key() -> Option<String> {
        keyring::Entry::new(SERVICE, ACCOUNT).ok()?.get_password().ok()
    }

    pub fn set_api_key(key: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())?;
        if key.trim().is_empty() {
            let _ = entry.delete_credential();
            Ok(())
        } else {
            entry.set_password(key.trim()).map_err(|e| e.to_string())
        }
    }

    pub async fn stream(model: &str, kind: &Kind, system: &str, user: &str, mut on_delta: impl FnMut(&str)) -> Result<(), String> {
        let key = api_key().ok_or("Falta la API key de Claude (⌘P → «Configurar API key de Claude»)")?;
        // Short command answers need little thinking; explanations a bit more.
        let effort = match kind {
            Kind::Command => "low",
            _ => "medium",
        };
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 16000,
            "stream": true,
            "system": system,
            "messages": [{"role": "user", "content": user}],
            "output_config": {"effort": effort},
            // Server-side fallback if a safety classifier declines the request.
            "fallbacks": "default",
        });
        let client = reqwest::Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", key)
            .header("anthropic-version", "2026-10-21")
            .header("anthropic-beta", "server-side-fallback-2026-07-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("No se pudo conectar con Claude: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let v: serde_json::Value = resp.json().await.unwrap_or_default();
            let msg = v["error"]["message"].as_str().unwrap_or("error desconocido");
            return Err(format!("Claude respondió {status}: {msg}"));
        }
        for_each_line(resp, |line| {
            let Some(data) = line.strip_prefix("data:") else { return Ok(false) };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(data.trim()) else { return Ok(false) };
            match v["type"].as_str() {
                Some("content_block_delta") if v["delta"]["type"] == "text_delta" => {
                    if let Some(t) = v["delta"]["text"].as_str() {
                        on_delta(t);
                    }
                    Ok(false)
                }
                Some("message_delta") if v["delta"]["stop_reason"] == "refusal" => {
                    Err("Claude rechazó esta solicitud.".into())
                }
                Some("error") => Err(v["error"]["message"].as_str().unwrap_or("error en el stream").to_string()),
                Some("message_stop") => Ok(true),
                _ => Ok(false),
            }
        })
        .await
    }
}

// ---------- selection ----------

fn cloud_available() -> bool {
    cfg!(feature = "cloud-ai")
}

/// (provider, model) currently selected; model may be empty for LM Studio
/// (= first loaded model).
fn selection(state: &AppState) -> (String, String, AiConfig) {
    let cfg = state.config.read().unwrap().ai.clone();
    let prefs = state.prefs.read().unwrap();
    let provider = prefs.ai_provider.clone();
    let model = if !prefs.ai_model.is_empty() {
        prefs.ai_model.clone()
    } else {
        match provider.as_str() {
            "claude" => cfg.claude_model.clone(),
            "lmstudio" => cfg.lmstudio_model.clone(),
            _ => cfg.ollama_model.clone(),
        }
    };
    (provider, model, cfg)
}

fn model_present(models: &[String], model: &str) -> bool {
    models.iter().any(|n| n == model || n == &format!("{model}:latest"))
}

// ---------- commands ----------

#[tauri::command]
pub async fn ai_status(state: State<'_, AppState>) -> Result<AiStatus, String> {
    let (provider, model, cfg) = selection(&state);
    let mk = |model: String, local: bool, ready: bool, detail: String| AiStatus {
        provider_label: provider_label(&provider).into(),
        provider: provider.clone(),
        model,
        local,
        ready,
        detail,
        cloud_available: cloud_available(),
    };
    Ok(match provider.as_str() {
        #[cfg(feature = "cloud-ai")]
        "claude" => {
            let has_key = claude::api_key().is_some();
            let detail = if has_key { "Listo · los datos salen a la API de Anthropic" } else { "Falta la API key (⌘P → Configurar API key de Claude)" };
            mk(model, false, has_key, detail.into())
        }
        "lmstudio" => match lmstudio_models(&cfg.lmstudio_url).await {
            Err(e) => mk(model, true, false, e),
            Ok(models) if models.is_empty() => mk(model, true, false, "LM Studio no tiene modelos cargados. Carga uno en la app.".into()),
            Ok(models) => {
                if model.is_empty() {
                    mk(models[0].clone(), true, true, "Listo".into())
                } else if models.contains(&model) {
                    mk(model, true, true, "Listo".into())
                } else {
                    mk(model.clone(), true, false, format!("El modelo «{model}» no está cargado en LM Studio"))
                }
            }
        },
        _ => match ollama_models(&cfg.ollama_url).await {
            Err(e) => mk(model, true, false, e),
            Ok(models) if model_present(&models, &model) => mk(model, true, true, "Listo".into()),
            Ok(_) => mk(model.clone(), true, false, format!("Falta el modelo. Descárgalo con: ollama pull {model}")),
        },
    })
}

/// Every model the user can pick, grouped by provider.
#[tauri::command]
pub async fn ai_list_models(state: State<'_, AppState>) -> Result<ModelCatalog, String> {
    let (provider, model, cfg) = selection(&state);
    let (ollama, lms) = tokio::join!(ollama_models(&cfg.ollama_url), lmstudio_models(&cfg.lmstudio_url));
    let info = |id, label, r: Result<Vec<String>, String>| match r {
        Ok(models) => ProviderInfo {
            id,
            label,
            local: true,
            available: true,
            detail: if models.is_empty() { "Sin modelos".into() } else { format!("{} modelos", models.len()) },
            models,
        },
        Err(detail) => ProviderInfo { id, label, local: true, available: false, detail, models: vec![] },
    };
    #[allow(unused_mut)]
    let mut providers = vec![info("ollama", "Ollama", ollama), info("lmstudio", "LM Studio", lms)];
    #[cfg(feature = "cloud-ai")]
    {
        let mut models: Vec<String> = CLAUDE_MODELS.iter().map(|s| s.to_string()).collect();
        if !models.contains(&cfg.claude_model) {
            models.insert(0, cfg.claude_model.clone());
        }
        let has_key = claude::api_key().is_some();
        providers.push(ProviderInfo {
            id: "claude",
            label: "Claude (nube)",
            local: false,
            available: has_key,
            detail: if has_key { "Los datos salen a Anthropic".into() } else { "Falta la API key".into() },
            models,
        });
    }
    Ok(ModelCatalog { providers, provider, model })
}

#[tauri::command]
pub fn ai_select_model(state: State<'_, AppState>, provider: String, model: String) -> Result<(), String> {
    match provider.as_str() {
        "ollama" | "lmstudio" => {}
        "claude" if cloud_available() => {}
        "claude" => return Err("Esta compilación es 100% local (sin la feature cloud-ai).".into()),
        _ => return Err("Proveedor desconocido".into()),
    }
    let mut prefs = state.prefs.write().unwrap();
    prefs.ai_provider = provider;
    prefs.ai_model = model;
    let clean = prefs.clone().sanitized(cloud_available());
    *prefs = clean;
    crate::prefs::save(&state.data_dir, &prefs);
    Ok(())
}

#[tauri::command]
pub fn ai_set_api_key(key: String) -> Result<(), String> {
    #[cfg(feature = "cloud-ai")]
    {
        claude::set_api_key(&key)
    }
    #[cfg(not(feature = "cloud-ai"))]
    {
        let _ = key;
        Err("Esta compilación es 100% local (sin la feature cloud-ai).".into())
    }
}

#[tauri::command]
pub fn ai_ask(state: State<'_, AppState>, request: AiRequest, on_event: Channel<AiEvent>) -> u32 {
    let (provider, model, cfg) = selection(&state);
    let id = state.next_ai.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let (system, user, redactions) = build_messages(&request, cfg.max_context_chars);
    let local = provider != "claude";
    let _ = on_event.send(AiEvent::Context {
        sent: user.clone(),
        redactions,
        provider: provider.clone(),
        model: model.clone(),
        local,
    });

    let tasks = state.ai_tasks.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let send = |t: &str| {
            let _ = on_event.send(AiEvent::Delta { text: t.to_string() });
        };
        let result = match provider.as_str() {
            "lmstudio" => {
                // No model chosen: use whichever model LM Studio has loaded.
                let model = if model.is_empty() {
                    lmstudio_models(&cfg.lmstudio_url).await.ok().and_then(|m| m.into_iter().next()).unwrap_or_default()
                } else {
                    model
                };
                if model.is_empty() {
                    Err("LM Studio no tiene ningún modelo cargado".into())
                } else {
                    lmstudio_stream(&cfg.lmstudio_url, &model, system, &user, send).await
                }
            }
            #[cfg(feature = "cloud-ai")]
            "claude" => claude::stream(&model, &request.kind, system, &user, send).await,
            _ => ollama_stream(&cfg.ollama_url, &model, system, &user, send).await,
        };
        let _ = on_event.send(match result {
            Ok(()) => AiEvent::Done,
            Err(message) => AiEvent::Error { message },
        });
        tasks.lock().unwrap().remove(&id);
    });
    state.ai_tasks.lock().unwrap().insert(id, handle);
    id
}

#[tauri::command]
pub fn ai_cancel(state: State<'_, AppState>, id: u32) {
    if let Some(h) = state.ai_tasks.lock().unwrap().remove(&id) {
        h.abort();
    }
}

/// Reasons a command looks dangerous (empty = fine).
#[tauri::command]
pub fn check_command(cmd: String) -> Vec<String> {
    crate::danger::check(&cmd).into_iter().map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn only_loopback_urls() {
        assert!(is_loopback_url("http://127.0.0.1:11434"));
        assert!(is_loopback_url("http://localhost:1234"));
        assert!(is_loopback_url("http://[::1]:11434"));
        assert!(!is_loopback_url("http://192.168.1.10:11434"));
        assert!(!is_loopback_url("https://ollama.example.com"));
        assert!(!is_loopback_url("http://127.0.0.1.evil.com"));
    }

    /// Minimal HTTP server answering each request with a canned body.
    fn mock_server(responses: Vec<(&'static str, String)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for (ctype, body) in responses {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0u8; 8192];
                let _ = s.read(&mut buf);
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {ctype}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes());
            }
        });
        format!("http://127.0.0.1:{}", addr.port())
    }

    #[test]
    fn lmstudio_openai_compatible_api() {
        let sse = [
            r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#,
            r#"data: {"choices":[{"delta":{"content":"lsof "}}]}"#,
            r#"data: {"choices":[{"delta":{"content":"-i :3000"}}]}"#,
            "data: [DONE]",
        ]
        .join("\n\n");
        let url = mock_server(vec![
            ("application/json", r#"{"data":[{"id":"qwen2.5-coder-7b-instruct"},{"id":"llama-3.2-3b"}]}"#.into()),
            ("text/event-stream", sse),
        ]);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let models = lmstudio_models(&url).await.unwrap();
            assert_eq!(models, vec!["qwen2.5-coder-7b-instruct", "llama-3.2-3b"]);
            let mut out = String::new();
            lmstudio_stream(&url, &models[0], "sys", "user", |t| out.push_str(t)).await.unwrap();
            assert_eq!(out, "lsof -i :3000");
        });
    }

    #[test]
    fn lmstudio_rejects_remote_urls() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            assert!(lmstudio_models("http://10.0.0.5:1234").await.is_err());
            assert!(lmstudio_stream("https://api.example.com", "m", "s", "u", |_| {}).await.is_err());
        });
    }

    /// Needs a running Ollama with the default model: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn ollama_end_to_end() {
        let cfg = crate::config::AiConfig::default();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let models = ollama_models(&cfg.ollama_url).await.unwrap();
            assert!(model_present(&models, &cfg.ollama_model), "falta {}", cfg.ollama_model);
            for (kind, prompt, ctx) in [
                (Kind::Command, "muestra qué proceso usa el puerto 3000", None),
                (Kind::Command, "busca archivos .log de más de 100MB en esta carpeta", None),
                (
                    Kind::Explain,
                    "Explica por qué falló este comando y cómo corregirlo.",
                    Some("Comando: git pusj\nCódigo de salida: 1\nSalida:\ngit: 'pusj' is not a git command. See 'git --help'.\n\nThe most similar command is\n\tpush"),
                ),
            ] {
                let req = AiRequest { kind: kind.clone(), prompt: prompt.into(), context: ctx.map(String::from), cwd: Some("~/proyecto".into()) };
                let (system, user, _) = build_messages(&req, cfg.max_context_chars);
                let mut out = String::new();
                let t0 = std::time::Instant::now();
                ollama_stream(&cfg.ollama_url, &cfg.ollama_model, system, &user, |t| out.push_str(t)).await.unwrap();
                println!("--- {kind:?} ({:.1}s) «{prompt}»\n{out}\n", t0.elapsed().as_secs_f32());
                assert!(!out.trim().is_empty());
            }
        });
    }

    #[test]
    fn context_is_redacted_and_trimmed() {
        let req = AiRequest {
            kind: Kind::Explain,
            prompt: "¿qué pasó?".into(),
            context: Some(format!("{}\nexport TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789", "x".repeat(10_000))),
            cwd: Some("/tmp".into()),
        };
        let (_, user, n) = build_messages(&req, 200);
        assert!(n >= 1);
        assert!(!user.contains("ghp_abcdefghijklmnop"));
        assert!(user.contains("[…recortado…]"));
        assert!(user.len() < 400);
    }
}
