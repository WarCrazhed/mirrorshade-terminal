import type { ITheme } from "@xterm/xterm";

/**
 * Color themes. Each one defines the 16 ANSI colors for the terminal; the UI
 * palette (sidebar, panels, borders…) is derived from bg/fg/accent with CSS
 * color-mix, so a theme only needs a handful of values.
 */
export interface Theme {
  id: string;
  name: string;
  light: boolean;
  /** Created by the user (saved in themes.json). */
  custom?: boolean;
  bg: string;
  fg: string;
  accent: string;
  cursor: string;
  selection: string;
  ansi: [string, string, string, string, string, string, string, string, string, string, string, string, string, string, string, string];
}

const t = (
  id: string,
  name: string,
  light: boolean,
  bg: string,
  fg: string,
  accent: string,
  cursor: string,
  selection: string,
  ansi: Theme["ansi"],
): Theme => ({ id, name, light, bg, fg, accent, cursor, selection, ansi });

export const THEMES: Theme[] = [
  t("tokyo-night", "Tokyo Night", false, "#0f1117", "#d6dae4", "#7aa2f7", "#7aa2f7", "#2f3a55", [
    "#1a1d26", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5",
    "#565f89", "#ff8fa3", "#b4e88a", "#f0c88a", "#9ab8ff", "#cdb4ff", "#a4e0ff", "#e6ebff",
  ]),
  t("catppuccin", "Catppuccin Mocha", false, "#1e1e2e", "#cdd6f4", "#89b4fa", "#f5e0dc", "#45475a", [
    "#45475a", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#bac2de",
    "#585b70", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#a6adc8",
  ]),
  t("dracula", "Dracula", false, "#282a36", "#f8f8f2", "#bd93f9", "#f8f8f2", "#44475a", [
    "#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2",
    "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff",
  ]),
  t("nord", "Nord", false, "#2e3440", "#d8dee9", "#88c0d0", "#d8dee9", "#434c5e", [
    "#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0",
    "#4c566a", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4",
  ]),
  t("gruvbox", "Gruvbox", false, "#282828", "#ebdbb2", "#fabd2f", "#ebdbb2", "#504945", [
    "#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984",
    "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2",
  ]),
  t("one-dark", "One Dark", false, "#282c34", "#abb2bf", "#61afef", "#528bff", "#3e4451", [
    "#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf",
    "#5c6370", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#ffffff",
  ]),
  t("rose-pine", "Rosé Pine", false, "#191724", "#e0def4", "#c4a7e7", "#524f67", "#2a283e", [
    "#26233a", "#eb6f92", "#31748f", "#f6c177", "#9ccfd8", "#c4a7e7", "#ebbcba", "#e0def4",
    "#6e6a86", "#eb6f92", "#31748f", "#f6c177", "#9ccfd8", "#c4a7e7", "#ebbcba", "#e0def4",
  ]),
  t("solarized-dark", "Solarized Oscuro", false, "#002b36", "#93a1a1", "#268bd2", "#93a1a1", "#073642", [
    "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
    "#586e75", "#cb4b16", "#859900", "#b58900", "#268bd2", "#6c71c4", "#2aa198", "#fdf6e3",
  ]),
  t("monokai", "Monokai", false, "#272822", "#f8f8f2", "#66d9ef", "#f8f8f0", "#49483e", [
    "#272822", "#f92672", "#a6e22e", "#f4bf75", "#66d9ef", "#ae81ff", "#a1efe4", "#f8f8f2",
    "#75715e", "#f92672", "#a6e22e", "#f4bf75", "#66d9ef", "#ae81ff", "#a1efe4", "#f9f8f5",
  ]),
  t("kanagawa", "Kanagawa", false, "#1f1f28", "#dcd7ba", "#7e9cd8", "#c8c093", "#2d4f67", [
    "#16161d", "#c34043", "#76946a", "#c0a36e", "#7e9cd8", "#957fb8", "#6a9589", "#c8c093",
    "#727169", "#e82424", "#98bb6c", "#e6c384", "#7fb4ca", "#938aa9", "#7aa89f", "#dcd7ba",
  ]),
  t("everforest", "Everforest", false, "#2d353b", "#d3c6aa", "#a7c080", "#d3c6aa", "#475258", [
    "#475258", "#e67e80", "#a7c080", "#dbbc7f", "#7fbbb3", "#d699b6", "#83c092", "#d3c6aa",
    "#859289", "#e67e80", "#a7c080", "#dbbc7f", "#7fbbb3", "#d699b6", "#83c092", "#e9e1cc",
  ]),
  t("night-owl", "Night Owl", false, "#011627", "#d6deeb", "#82aaff", "#80a4c2", "#1d3b53", [
    "#011627", "#ef5350", "#22da6e", "#addb67", "#82aaff", "#c792ea", "#21c7a8", "#ffffff",
    "#575656", "#ef5350", "#22da6e", "#ffeb95", "#82aaff", "#c792ea", "#7fdbca", "#ffffff",
  ]),
  t("ayu", "Ayu Oscuro", false, "#0b0e14", "#bfbdb6", "#ffb454", "#e6b450", "#273747", [
    "#1e232b", "#ea6c73", "#7fd962", "#f9af4f", "#53bdfa", "#cda1fa", "#90e1c6", "#c7c7c7",
    "#686868", "#f07178", "#aad94c", "#ffb454", "#59c2ff", "#d2a6ff", "#95e6cb", "#ffffff",
  ]),
  t("palenight", "Palenight", false, "#292d3e", "#a6accd", "#c792ea", "#ffcc00", "#444267", [
    "#292d3e", "#f07178", "#c3e88d", "#ffcb6b", "#82aaff", "#c792ea", "#89ddff", "#d0d0d0",
    "#676e95", "#ff8b92", "#ddffa7", "#ffe585", "#9cc4ff", "#e1acff", "#a3f7ff", "#ffffff",
  ]),
  t("synthwave", "Synthwave '84", false, "#262335", "#f4eee4", "#ff7edb", "#f97e72", "#463465", [
    "#262335", "#fe4450", "#72f1b8", "#fede5d", "#03edf9", "#ff7edb", "#03edf9", "#f4eee4",
    "#614d85", "#fe4450", "#72f1b8", "#f97e72", "#03edf9", "#ff7edb", "#03edf9", "#ffffff",
  ]),
  t("catppuccin-latte", "Catppuccin Latte", true, "#eff1f5", "#4c4f69", "#1e66f5", "#dc8a78", "#ccd0da", [
    "#5c5f77", "#d20f39", "#40a02b", "#df8e1d", "#1e66f5", "#ea76cb", "#179299", "#acb0be",
    "#6c6f85", "#d20f39", "#40a02b", "#df8e1d", "#1e66f5", "#ea76cb", "#179299", "#bcc0cc",
  ]),
  t("gruvbox-light", "Gruvbox Claro", true, "#fbf1c7", "#3c3836", "#076678", "#3c3836", "#ebdbb2", [
    "#3c3836", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#7c6f64",
    "#928374", "#9d0006", "#79740e", "#b57614", "#076678", "#8f3f71", "#427b58", "#282828",
  ]),
  t("one-light", "One Claro", true, "#fafafa", "#383a42", "#4078f2", "#526fff", "#e5e5e6", [
    "#383a42", "#e45649", "#50a14f", "#c18401", "#4078f2", "#a626a4", "#0184bc", "#a0a1a7",
    "#696c77", "#e45649", "#50a14f", "#c18401", "#4078f2", "#a626a4", "#0184bc", "#383a42",
  ]),
  t("solarized-light", "Solarized Claro", true, "#fdf6e3", "#586e75", "#268bd2", "#586e75", "#eee8d5", [
    "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#93a1a1",
    "#839496", "#cb4b16", "#859900", "#b58900", "#268bd2", "#6c71c4", "#2aa198", "#073642",
  ]),
  t("github-light", "GitHub Claro", true, "#ffffff", "#24292f", "#0969da", "#0969da", "#cce4ff", [
    "#24292f", "#cf222e", "#116329", "#9a6700", "#0969da", "#8250df", "#1b7c83", "#6e7781",
    "#8c959f", "#a40e26", "#1a7f37", "#7d4e00", "#218bff", "#a475f9", "#3192aa", "#24292f",
  ]),
];

/** Blend two #rrggbb colors (t = 0..1). */
export function mix(a: string, b: string, t: number): string {
  const p = (h: string, i: number) => parseInt(h.slice(i, i + 2), 16);
  const c = (i: number) => Math.round(p(a, i) + (p(b, i) - p(a, i)) * t).toString(16).padStart(2, "0");
  return `#${c(1)}${c(3)}${c(5)}`;
}

export function getTheme(id: string, custom: Theme[] = []): Theme {
  return THEMES.find((x) => x.id === id) ?? custom.find((x) => x.id === id) ?? THEMES[0];
}

/** Stable id for a new user theme: custom-<slug>-<random>. */
export function newThemeId(name: string): string {
  const slug = name
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 20);
  const rnd = Math.floor(Math.random() * 0xffff).toString(16).padStart(4, "0");
  return `custom-${slug || "tema"}-${rnd}`;
}

export function xtermTheme(th: Theme): ITheme {
  const [black, red, green, yellow, blue, magenta, cyan, white, bBlack, bRed, bGreen, bYellow, bBlue, bMagenta, bCyan, bWhite] = th.ansi;
  return {
    // Fully transparent (alpha 00): the window paints the (possibly
    // translucent) background once — painting it here too doubled the alpha.
    // The RGB part still matters: xterm blends dim text, inverse video and
    // the cursor with it. "#00000000" made those come out BLACK (e.g. the
    // dimmed placeholder in Claude Code's input box).
    background: th.bg + "00",
    foreground: th.fg,
    cursor: th.cursor,
    cursorAccent: th.bg,
    selectionBackground: th.selection,
    black, red, green, yellow, blue, magenta, cyan, white,
    brightBlack: bBlack, brightRed: bRed, brightGreen: bGreen, brightYellow: bYellow,
    brightBlue: bBlue, brightMagenta: bMagenta, brightCyan: bCyan, brightWhite: bWhite,
  };
}

/** WCAG relative luminance of #rrggbb. */
function luminance(hex: string): number {
  const ch = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
}

/** Black or white, whichever reads better on `bg`. */
export function textOn(bg: string): string {
  const l = luminance(bg);
  return (l + 0.05) / 0.05 >= 1.05 / (l + 0.05) ? "#000000" : "#ffffff";
}

/** Push the theme into CSS custom properties used by every component. */
export function applyUiTheme(th: Theme, opacity: number) {
  const s = document.documentElement.style;
  s.setProperty("--bg", th.bg);
  s.setProperty("--text", th.fg);
  s.setProperty("--accent", th.accent);
  s.setProperty("--ok", th.ansi[2]);
  s.setProperty("--err", th.ansi[1]);
  s.setProperty("--warn", th.ansi[3]);
  s.setProperty("--selection", th.selection);
  // Text on filled buttons: white on dark colors, black on light ones.
  s.setProperty("--on-accent", textOn(th.accent));
  s.setProperty("--on-err", textOn(th.ansi[1]));
  s.setProperty("--opacity", String(opacity));
  s.setProperty("color-scheme", th.light ? "light" : "dark");
}
