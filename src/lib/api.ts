import { Channel, invoke } from "@tauri-apps/api/core";
import type { Theme } from "./themes";

export interface Suggestion {
  label: string;
  insert: string;
  replace: number;
  kind: string;
  description: string;
}

export interface Prefs {
  theme: string;
  opacity: number;
  /** Background blur radius, 0 = off (max 64). */
  blur_radius: number;
  font_size: number;
  prompt_theme: "none" | "two-line" | "minimal" | "blocks";
  prompt_gradient: boolean;
  sidebar: boolean;
  sidebar_width: number;
  shell: "auto" | "zsh" | "bash" | "fish";
  ai_provider: string;
  ai_model: string;
}

export interface FrontendConfig {
  scrollback: number;
  font_size: number;
  font_family: string;
  config_path: string;
  prefs: Prefs;
}

export interface ProviderInfo {
  id: string;
  label: string;
  local: boolean;
  available: boolean;
  detail: string;
  models: string[];
}

export interface ModelCatalog {
  providers: ProviderInfo[];
  provider: string;
  model: string;
}

export interface AiStatus {
  provider: string;
  provider_label: string;
  model: string;
  local: boolean;
  ready: boolean;
  detail: string;
  cloud_available: boolean;
}

export interface TabView {
  id: number;
  title: string;
  cwd: string;
  /** Command currently running ("" when idle at the prompt). */
  running: string;
}

export interface Action {
  id: string;
  label: string;
  shortcut?: string;
  run: () => void;
}

export type AiKind = "command" | "explain" | "chat";

export type AiEvent =
  | { type: "context"; sent: string; redactions: number; provider: string; model: string; local: boolean }
  | { type: "delta"; text: string }
  | { type: "done" }
  | { type: "error"; message: string };

const markCustom = (list: Theme[]) => list.map((t) => ({ ...t, custom: true }));

export const api = {
  config: () => invoke<FrontendConfig>("get_config"),
  reloadConfig: () => invoke<FrontendConfig>("reload_config"),
  historyAdd: (cmd: string, cwd: string, exitCode: number) =>
    invoke<void>("history_add", { cmd, cwd, exitCode }),
  ghost: (input: string, cwd: string) => invoke<string | null>("complete_ghost", { input, cwd }),
  complete: (input: string, cwd: string, path: string, remote = false) =>
    invoke<Suggestion[]>("complete_list", { input, cwd, path, remote }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  openPath: (path: string, cwd: string, line?: number, col?: number) =>
    invoke<string>("open_path", { path, cwd, line: line ?? null, col: col ?? null }),
  pathsExist: (paths: string[], cwd: string) => invoke<boolean[]>("paths_exist", { paths, cwd }),
  shellsAvailable: () => invoke<string[]>("shells_available"),
  aiStatus: () => invoke<AiStatus>("ai_status"),
  prefsSet: (prefs: Prefs) => invoke<Prefs>("prefs_set", { prefs }),
  themesList: () => invoke<Theme[]>("themes_list").then(markCustom),
  themeSave: (theme: Theme) => {
    const { custom: _c, ...plain } = theme;
    return invoke<Theme[]>("theme_save", { theme: plain }).then(markCustom);
  },
  themeDelete: (id: string) => invoke<Theme[]>("theme_delete", { id }).then(markCustom),
  /** Push prompt style + theme colors to running shells (applies on next Enter). */
  promptSync: (colors: string[]) => invoke<void>("prompt_sync", { colors }),
  aiListModels: () => invoke<ModelCatalog>("ai_list_models"),
  aiSelectModel: (provider: string, model: string) => invoke<void>("ai_select_model", { provider, model }),
  aiSetApiKey: (key: string) => invoke<void>("ai_set_api_key", { key }),
  aiCancel: (id: number) => invoke<void>("ai_cancel", { id }),
  checkCommand: (cmd: string) => invoke<string[]>("check_command", { cmd }),
  aiAsk(
    request: { kind: AiKind; prompt: string; context?: string; cwd?: string },
    onEvent: (e: AiEvent) => void,
  ): Promise<number> {
    const ch = new Channel<AiEvent>();
    ch.onmessage = onEvent;
    return invoke<number>("ai_ask", { request, onEvent: ch });
  },
};

/**
 * Make AI- or user-supplied text safe to type into the shell: strip control
 * characters (escape sequences, \r that would execute, bracketed-paste end
 * markers) but keep newlines and tabs.
 */
export function sanitizeForShell(text: string): string {
  return text
    .replace(/\r\n?/g, "\n")
    .replace(/\t/g, " ")
    // eslint-disable-next-line no-control-regex
    .replace(/[\x00-\x08\x0b-\x1f\x7f-\x9f]/g, "")
    .replace(/\n+$/g, "");
}

/** Pull a shell command out of a model answer (strips fences, `$ ` prompts, quotes). */
export function extractCommand(answer: string): string {
  let t = answer.trim();
  const fence = t.match(/```[a-zA-Z]*\n?([\s\S]*?)```/);
  if (fence) t = fence[1].trim();
  t = t.replace(/^`+|`+$/g, "").trim();
  t = t
    .split("\n")
    .map((l) => l.replace(/^\s*[$%>]\s+/, ""))
    .filter((l) => l.trim() !== "")
    .join("\n");
  return t;
}

/** Split a markdown-ish answer into text and code segments (rendered as plain text, never HTML). */
export function splitCode(answer: string): { code: boolean; text: string }[] {
  const out: { code: boolean; text: string }[] = [];
  const re = /```[a-zA-Z]*\n?([\s\S]*?)(```|$)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(answer))) {
    if (m.index > last) out.push({ code: false, text: answer.slice(last, m.index) });
    out.push({ code: true, text: m[1].replace(/\n$/, "") });
    last = re.lastIndex;
    if (m[0].length === 0) break;
  }
  if (last < answer.length) out.push({ code: false, text: answer.slice(last) });
  return out.filter((s) => s.text.trim() !== "");
}
