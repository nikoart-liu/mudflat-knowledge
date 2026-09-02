import { describe, expect, it } from 'vitest';
import { layoutMindmap, nodeBox } from './mindmap-layout';
import type { MindmapNode } from './types';

function theme(id: string, label: string, children: MindmapNode[] = []): MindmapNode {
  return { id, label, kind: 'theme', sourceCardIds: [1, 2], children };
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
      expect(len).toBeGreaterThan(40);
      expect(e.y1).toBeGreaterThan(0);
    }
  });

  it('中心节点比二级更宽', () => {
    const hub = nodeBox('原子习惯 · 我的划线', 0);
    const leaf = nodeBox('把充电器换房间', 2);
    expect(hub.w).toBeGreaterThan(leaf.w);
  });
});
