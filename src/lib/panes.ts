/**
 * Split-pane layout as an immutable binary tree. Terminals are rendered in a
 * flat, keyed list and positioned from `layout()`, so splitting or closing a
 * pane never re-creates (and kills) the other panes' shells.
 */

export type PaneNode =
  | { kind: "pane"; id: number }
  | { kind: "split"; dir: "row" | "col"; ratio: number; a: PaneNode; b: PaneNode };

/** Fractions (0..1) of the tab area. */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Divider {
  dir: "row" | "col";
  /** Area covered by the split this divider belongs to. */
  rect: Rect;
  ratio: number;
  /** Path (a=0 / b=1) from the root to the split node. */
  path: number[];
}

export function leaf(id: number): PaneNode {
  return { kind: "pane", id };
}

export function paneIds(node: PaneNode): number[] {
  return node.kind === "pane" ? [node.id] : [...paneIds(node.a), ...paneIds(node.b)];
}

export function layout(root: PaneNode) {
  const panes = new Map<number, Rect>();
  const dividers: Divider[] = [];
  const walk = (n: PaneNode, r: Rect, path: number[]) => {
    if (n.kind === "pane") {
      panes.set(n.id, r);
      return;
    }
    dividers.push({ dir: n.dir, rect: r, ratio: n.ratio, path });
    if (n.dir === "row") {
      walk(n.a, { x: r.x, y: r.y, w: r.w * n.ratio, h: r.h }, [...path, 0]);
      walk(n.b, { x: r.x + r.w * n.ratio, y: r.y, w: r.w * (1 - n.ratio), h: r.h }, [...path, 1]);
    } else {
      walk(n.a, { x: r.x, y: r.y, w: r.w, h: r.h * n.ratio }, [...path, 0]);
      walk(n.b, { x: r.x, y: r.y + r.h * n.ratio, w: r.w, h: r.h * (1 - n.ratio) }, [...path, 1]);
    }
  };
  walk(root, { x: 0, y: 0, w: 1, h: 1 }, []);
  return { panes, dividers };
}

/** Replace pane `id` with a split of it and a new pane `newId`. */
export function split(root: PaneNode, id: number, dir: "row" | "col", newId: number): PaneNode {
  if (root.kind === "pane") {
    return root.id === id ? { kind: "split", dir, ratio: 0.5, a: root, b: leaf(newId) } : root;
  }
  return { ...root, a: split(root.a, id, dir, newId), b: split(root.b, id, dir, newId) };
}

/** Remove pane `id`; its sibling takes the space. Null if it was the last pane. */
export function remove(root: PaneNode, id: number): PaneNode | null {
  if (root.kind === "pane") return root.id === id ? null : root;
  const a = remove(root.a, id);
  const b = remove(root.b, id);
  if (!a) return b;
  if (!b) return a;
  return { ...root, a, b };
}

export function setRatio(root: PaneNode, path: number[], ratio: number): PaneNode {
  if (root.kind === "pane") return root;
  if (path.length === 0) return { ...root, ratio: Math.min(0.85, Math.max(0.15, ratio)) };
  const [head, ...rest] = path;
  return head === 0 ? { ...root, a: setRatio(root.a, rest, ratio) } : { ...root, b: setRatio(root.b, rest, ratio) };
}

/** Closest pane in a direction from pane `id` (for ⌘⌥ + arrows). */
export function neighbor(root: PaneNode, id: number, dir: "left" | "right" | "up" | "down"): number | undefined {
  const { panes } = layout(root);
  const from = panes.get(id);
  if (!from) return undefined;
  const cx = from.x + from.w / 2;
  const cy = from.y + from.h / 2;
  const eps = 1e-6;
  let best: { id: number; d: number } | undefined;
  for (const [pid, r] of panes) {
    if (pid === id) continue;
    const ok =
      dir === "left" ? r.x + r.w <= from.x + eps :
      dir === "right" ? r.x >= from.x + from.w - eps :
      dir === "up" ? r.y + r.h <= from.y + eps :
      r.y >= from.y + from.h - eps;
    if (!ok) continue;
    const d = Math.hypot(r.x + r.w / 2 - cx, r.y + r.h / 2 - cy);
    if (!best || d < best.d) best = { id: pid, d };
  }
  return best?.id;
}
