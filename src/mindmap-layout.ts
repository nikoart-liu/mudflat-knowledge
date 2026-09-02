import type { MindmapNode } from './types';

export type LaidNode = {
  id: string;
  depth: number;
  x: number;
  y: number;
  w: number;
  h: number;
  node: MindmapNode;
};

export type LaidEdge = {
  from: string;
  to: string;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
};

export type MindmapLayout = {
  width: number;
  height: number;
  nodes: LaidNode[];
  edges: LaidEdge[];
};

const PAD = 48;

function charCount(s: string): number {
  return [...s].length;
}

export function nodeBox(label: string, depth: number): { w: number; h: number } {
  const n = Math.max(1, charCount(label));
  if (depth === 0) {
    const w = Math.min(280, Math.max(168, n * 16 + 32));
    const h = n > 10 ? 72 : 56;
    return { w, h };
  }
  const w = Math.min(220, Math.max(132, n * 13 + 36));
  const lines = Math.ceil((n * 13) / (w - 28));
  const h = Math.max(48, 18 + lines * 22);
  return { w, h };
}

function centerOf(n: LaidNode): { x: number; y: number } {
  return { x: n.x + n.w / 2, y: n.y + n.h / 2 };
}

/// 中心辐射：根在中间，一级沿椭圆均分，二级沿同一方向再外扩。
export function layoutMindmap(root: MindmapNode): MindmapLayout {
  const nodes: LaidNode[] = [];
  const rootBox = nodeBox(root.label, 0);
  const n = root.children.length;
  const rx = Math.max(250, 58 * Math.max(n, 3));
  const ry = Math.max(170, 42 * Math.max(n, 3));
  const cx = rx + 220;
  const cy = ry + 160;

  nodes.push({
    id: root.id,
    depth: 0,
    x: cx - rootBox.w / 2,
    y: cy - rootBox.h / 2,
    w: rootBox.w,
    h: rootBox.h,
    node: root,
  });

  root.children.forEach((child, i) => {
    const angle = n === 1 ? 0 : -Math.PI / 2 + (2 * Math.PI * i) / n;
    const box = nodeBox(child.label, 1);
    const px = cx + Math.cos(angle) * rx;
    const py = cy + Math.sin(angle) * ry;
    nodes.push({
      id: child.id,
      depth: 1,
      x: px - box.w / 2,
      y: py - box.h / 2,
      w: box.w,
      h: box.h,
      node: child,
    });
    const k = child.children.length;
    child.children.forEach((grand, j) => {
      const spread = k <= 1 ? 0 : (j - (k - 1) / 2) * 0.28;
      const a2 = angle + spread;
      const box2 = nodeBox(grand.label, 2);
      const px2 = cx + Math.cos(a2) * (rx + 168);
      const py2 = cy + Math.sin(a2) * (ry + 112);
      nodes.push({
        id: grand.id,
        depth: 2,
        x: px2 - box2.w / 2,
        y: py2 - box2.h / 2,
        w: box2.w,
        h: box2.h,
        node: grand,
      });
    });
  });

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const p of nodes) {
    minX = Math.min(minX, p.x);
    minY = Math.min(minY, p.y);
    maxX = Math.max(maxX, p.x + p.w);
    maxY = Math.max(maxY, p.y + p.h);
  }
  const dx = PAD - minX;
  const dy = PAD - minY;
  for (const p of nodes) {
    p.x += dx;
    p.y += dy;
  }

  const byId = new Map(nodes.map((p) => [p.id, p]));
  const edges: LaidEdge[] = [];
  const walk = (parent: MindmapNode) => {
    const a = byId.get(parent.id);
    if (!a) return;
    for (const child of parent.children) {
      const b = byId.get(child.id);
      if (b) {
        const ac = centerOf(a);
        const bc = centerOf(b);
        edges.push({ from: a.id, to: b.id, x1: ac.x, y1: ac.y, x2: bc.x, y2: bc.y });
      }
      walk(child);
    }
  };
  walk(root);

  return {
    width: Math.ceil(maxX - minX + PAD * 2),
    height: Math.ceil(maxY - minY + PAD * 2),
    nodes,
    edges,
  };
}
