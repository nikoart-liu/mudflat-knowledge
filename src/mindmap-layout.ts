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

export function visibleClue(root: MindmapNode, expandedId: string | null): MindmapNode {
  return {
    ...root,
    children: root.children.map((child) => ({
      ...child,
      children: child.id === expandedId ? child.children : [],
    })),
  };
}

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

function clipToBoxEdge(from: { x: number; y: number }, to: { x: number; y: number }, box: LaidNode): { x: number; y: number } {
  const vx = to.x - from.x;
  const vy = to.y - from.y;
  if (vx === 0 && vy === 0) return from;
  const hw = box.w / 2;
  const hh = box.h / 2;
  const sx = vx === 0 ? Infinity : hw / Math.abs(vx);
  const sy = vy === 0 ? Infinity : hh / Math.abs(vy);
  const t = Math.min(sx, sy);
  return { x: from.x + t * vx, y: from.y + t * vy };
}

const BOX_GAP = 24;

function boxesOverlap(a: LaidNode, b: LaidNode): boolean {
  return !(a.x + a.w + BOX_GAP <= b.x || b.x + b.w + BOX_GAP <= a.x
    || a.y + a.h + BOX_GAP <= b.y || b.y + b.h + BOX_GAP <= a.y);
}

function anyOverlap(nodes: LaidNode[]): boolean {
  for (let i = 0; i < nodes.length; i++) {
    for (let j = i + 1; j < nodes.length; j++) {
      if (boxesOverlap(nodes[i], nodes[j])) return true;
    }
  }
  return false;
}

const EXPAND_ROW = 22;

/// 中心辐射：根在中间，一级沿椭圆均分，二级沿同一方向再外扩。
export function layoutMindmap(root: MindmapNode, expandIds?: ReadonlySet<string>): MindmapLayout {
  const rootBox = nodeBox(root.label, 0);
  const n = root.children.length;
  let rx = Math.max(220, 48 * Math.max(n, 3));
  let ry = Math.max(150, 34 * Math.max(n, 3));
  let r2x = 168;
  let r2y = 112;
  let nodes: LaidNode[] = [];

  for (let iter = 0; iter < 24; iter++) {
    const cx = rx + 220;
    const cy = ry + 160;
    nodes = [{
      id: root.id,
      depth: 0,
      x: cx - rootBox.w / 2,
      y: cy - rootBox.h / 2,
      w: rootBox.w,
      h: rootBox.h,
      node: root,
    }];
    root.children.forEach((child, i) => {
      const angle = n === 1 ? 0 : -Math.PI / 2 + (2 * Math.PI * i) / n;
      const box = nodeBox(child.label, 1);
      const extra = (child.children.length > 0 || expandIds?.has(child.id)) ? EXPAND_ROW : 0;
      const h = box.h + extra;
      const px = cx + Math.cos(angle) * rx;
      const py = cy + Math.sin(angle) * ry;
      nodes.push({
        id: child.id,
        depth: 1,
        x: px - box.w / 2,
        y: py - h / 2,
        w: box.w,
        h,
        node: child,
      });
      const k = child.children.length;
      child.children.forEach((grand, j) => {
        const spread = k <= 1 ? 0 : (j - (k - 1) / 2) * 0.28;
        const a2 = angle + spread;
        const box2 = nodeBox(grand.label, 2);
        const px2 = cx + Math.cos(a2) * (rx + r2x);
        const py2 = cy + Math.sin(a2) * (ry + r2y);
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
    if (!anyOverlap(nodes)) break;
    rx *= 1.12;
    ry *= 1.12;
    r2x *= 1.12;
    r2y *= 1.12;
  }

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
        const p1 = clipToBoxEdge(ac, bc, a);
        const p2 = clipToBoxEdge(bc, ac, b);
        edges.push({ from: a.id, to: b.id, x1: p1.x, y1: p1.y, x2: p2.x, y2: p2.y });
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

export function exportClueOutline(map: { title: string; root: MindmapNode }): string {
  const lines: string[] = [`# 线索 · ${map.title}`];
  if (map.root.label) {
    lines.push(`\n中心：${map.root.label}\n`);
  }
  for (const c1 of map.root.children) {
    const sum1 = c1.summary ? ` —— ${c1.summary}` : '';
    const n1 = c1.sourceCardIds.length;
    lines.push(`- **${c1.label}**${sum1}${n1 > 0 ? ` (${n1}条证据)` : ''}`);
    for (const c2 of c1.children) {
      const sum2 = c2.summary ? ` —— ${c2.summary}` : '';
      const n2 = c2.sourceCardIds.length;
      lines.push(`  - ${c2.label}${sum2}${n2 > 0 ? ` (${n2}条证据)` : ''}`);
    }
  }
  return lines.join('\n');
}
