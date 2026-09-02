import { describe, expect, it } from 'vitest';
import { layoutMindmap, nodeBox, visibleClue } from './mindmap-layout';
import type { MindmapNode } from './types';

function theme(id: string, label: string, children: MindmapNode[] = []): MindmapNode {
  return { id, label, kind: 'theme', sourceCardIds: [1, 2], children };
}

function onBoxEdge(
  px: number, py: number,
  box: { x: number; y: number; w: number; h: number },
  eps = 0.6,
): boolean {
  const onX = Math.abs(px - box.x) <= eps || Math.abs(px - (box.x + box.w)) <= eps;
  const onY = Math.abs(py - box.y) <= eps || Math.abs(py - (box.y + box.h)) <= eps;
  const inX = px >= box.x - eps && px <= box.x + box.w + eps;
  const inY = py >= box.y - eps && py <= box.y + box.h + eps;
  return inX && inY && (onX || onY);
}

describe('layoutMindmap', () => {
  it('把根放在图的中部，一级主题用线连上', () => {
    const root: MindmapNode = {
      id: 'root',
      label: '原子习惯 · 我的划线',
      kind: 'book',
      sourceCardIds: [],
      children: [
        theme('a', '身份由重复塑造'),
        theme('b', '环境在替你做决定', [theme('b1', '把充电器换房间')]),
        theme('c', '先出现再优化动作'),
      ],
    };
    const laid = layoutMindmap(root);
    const hub = laid.nodes.find((n) => n.id === 'root');
    expect(hub).toBeTruthy();
    expect(hub!.depth).toBe(0);
    expect(hub!.x).toBeGreaterThan(40);
    expect(hub!.x + hub!.w).toBeLessThan(laid.width - 40);
    expect(laid.nodes.filter((n) => n.depth === 1)).toHaveLength(3);
    expect(laid.nodes.some((n) => n.id === 'b1' && n.depth === 2)).toBe(true);
    expect(laid.edges).toEqual(expect.arrayContaining([
      expect.objectContaining({ from: 'root', to: 'a' }),
      expect.objectContaining({ from: 'root', to: 'b' }),
      expect.objectContaining({ from: 'b', to: 'b1' }),
    ]));
    for (const e of laid.edges) {
      const len = Math.hypot(e.x2 - e.x1, e.y2 - e.y1);
      expect(len).toBeGreaterThan(0);
      expect(e.y1).toBeGreaterThan(0);
    }
  });

  it('线接到盒边，不穿心', () => {
    const root: MindmapNode = {
      id: 'root',
      label: '原子习惯 · 我的划线',
      kind: 'book',
      sourceCardIds: [],
      children: [
        theme('a', '身份由重复塑造'),
        theme('b', '环境在替你做决定', [theme('b1', '把充电器换房间')]),
        theme('c', '先出现再优化动作'),
      ],
    };
    const laid = layoutMindmap(root);
    const byId = new Map(laid.nodes.map((n) => [n.id, n]));
    expect(laid.edges.length).toBeGreaterThan(0);
    for (const e of laid.edges) {
      const from = byId.get(e.from)!;
      const to = byId.get(e.to)!;
      expect(onBoxEdge(e.x1, e.y1, from)).toBe(true);
      expect(onBoxEdge(e.x2, e.y2, to)).toBe(true);
      const fc = { x: from.x + from.w / 2, y: from.y + from.h / 2 };
      expect(Math.hypot(e.x1 - fc.x, e.y1 - fc.y)).toBeGreaterThan(8);
    }
  });

  it('节点盒子不相交', () => {
    const long = '这是一句相当长的一级概要需要两行';
    const root: MindmapNode = {
      id: 'root',
      label: '书名',
      kind: 'book',
      sourceCardIds: [],
      children: Array.from({ length: 12 }, (_, i) => theme(
        `t${i}`,
        long,
        i === 3
          ? [theme('t3a', '子题甲的归纳'), theme('t3b', '子题乙的归纳'), theme('t3c', '子题丙的归纳')]
          : [],
      )),
    };
    const laid = layoutMindmap(root);
    for (let i = 0; i < laid.nodes.length; i++) {
      for (let j = i + 1; j < laid.nodes.length; j++) {
        const a = laid.nodes[i];
        const b = laid.nodes[j];
        const separate = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
        expect(separate, `${a.id} overlaps ${b.id}`).toBe(true);
      }
    }
  });

  it('有子题的一级为展开行留高', () => {
    const root: MindmapNode = {
      id: 'root',
      label: '书名',
      kind: 'book',
      sourceCardIds: [],
      children: [
        theme('a', '身份由重复塑造'),
        theme('b', '环境在替你做决定', [theme('b1', '把充电器换房间')]),
      ],
    };
    const vis = visibleClue(root, null);
    const laid = layoutMindmap(vis, new Set(['b']));
    const a = laid.nodes.find((n) => n.id === 'a')!;
    const b = laid.nodes.find((n) => n.id === 'b')!;
    expect(b.h).toBe(a.h + 22);
    expect(laid.nodes.some((n) => n.id === 'b1')).toBe(false);
  });

  it('中心节点比二级更宽', () => {
    const hub = nodeBox('原子习惯 · 我的划线', 0);
    const leaf = nodeBox('把充电器换房间', 2);
    expect(hub.w).toBeGreaterThan(leaf.w);
  });
});

describe('visibleClue', () => {
  const root: MindmapNode = {
    id: 'root',
    label: '原子习惯 · 我的划线',
    kind: 'book',
    sourceCardIds: [],
    children: [
      theme('a', '身份由重复塑造'),
      theme('b', '环境在替你做决定', [theme('b1', '把充电器换房间')]),
      theme('c', '先出现再优化动作', [theme('c1', '两分钟起步'), theme('c2', '习惯叠加')]),
    ],
  };

  it('默认只含书名和一级', () => {
    const vis = visibleClue(root, null);
    expect(vis.children.map((c) => c.id)).toEqual(['a', 'b', 'c']);
    expect(vis.children.find((c) => c.id === 'b')!.children).toEqual([]);
    expect(vis.children.find((c) => c.id === 'c')!.children).toEqual([]);
  });

  it('展开某一级时只带出那一枝二级', () => {
    const vis = visibleClue(root, 'c');
    expect(vis.children.find((c) => c.id === 'c')!.children.map((g) => g.id)).toEqual(['c1', 'c2']);
    expect(vis.children.find((c) => c.id === 'b')!.children).toEqual([]);
  });
});
