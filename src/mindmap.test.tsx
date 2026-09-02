import { describe, expect, it, vi, beforeEach, afterEach, type Mock } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MindmapView } from './App';
import { call, type CardRow, type Mindmap } from './types';

afterEach(() => cleanup());

vi.mock('./types', async () => {
  const actual = await vi.importActual<typeof import('./types')>('./types');
  return { ...actual, call: vi.fn() };
});

const callMock = call as unknown as Mock;

const NOW = 1_700_000_000;
function card(id: number, text: string): CardRow {
  return {
    id, kind: 'highlight', bookId: 1, remoteId: `r-${id}`, chapterUid: 1,
    chapterTitle: '环境', text, abstractText: null, rangeStr: null, note: '',
    starred: false, excludedFromReview: false, createdAt: NOW, updatedAt: NOW,
    deleted: false, bookTitle: '原子习惯', tags: [],
  };
}

const map: Mindmap = {
  bookId: 1,
  title: '原子习惯',
  mode: 'theme',
  inputHash: 'h',
  promptVersion: 'mindmap-theme-v1',
  stats: { cards: 4, chapters: 1, themes: 1, unplaced: 0 },
  root: {
    id: 'root',
    label: '原子习惯 · 我的划线',
    kind: 'book',
    sourceCardIds: [],
    children: [{
      id: 't-env',
      label: '环境在替你做决定',
      kind: 'theme',
      summary: '少靠自控，多改摆设。',
      sourceCardIds: [8, 9],
      children: [],
    }],
  },
  warnings: [],
};

describe('MindmapView', () => {
  beforeEach(() => callMock.mockReset());

  it('未启用语言模型时引导去设置，不请求生成', async () => {
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: false, providerOff: true, cardCount: 18, cached: null, stale: false };
      }
      throw new Error(`未处理 ${cmd}`);
    });
    const toSettings: string[] = [];
    render(
      <MindmapView
        book={{ id: 1, title: '原子习惯' }}
        cards={[]}
        onToast={() => {}}
        onExit={() => {}}
        onOpenCard={() => {}}
        onNeedSettings={() => toSettings.push('go')}
      />,
    );
    await waitFor(() => expect(screen.getByText('还没有启用语言模型')).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: '去设置' }));
    expect(toSettings).toEqual(['go']);
    expect(callMock.mock.calls.some((c) => c[0] === 'generate_mindmap')).toBe(false);
  });

  it('点生成线索会调用 generate_mindmap', async () => {
    const generated = { ...map, inputHash: 'fresh' };
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return {
          available: true, providerOff: false, cardCount: 4, cached: null, stale: false,
          chatEndpoint: 'https://api.deepseek.com/v1/chat/completions', model: 'deepseek-chat',
        };
      }
      if (cmd === 'generate_mindmap') return generated;
      throw new Error(`未处理 ${cmd}`);
    });
    render(
      <MindmapView
        book={{ id: 1, title: '原子习惯' }}
        cards={[card(8, '环境是无形的手。'), card(9, '把书放在沙发上。')]}
        onToast={() => {}}
        onExit={() => {}}
        onOpenCard={() => {}}
        onNeedSettings={() => {}}
      />,
    );
    await waitFor(() => expect(screen.getByText(/POST https:\/\/api.deepseek.com\/v1\/chat\/completions/)).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: '生成线索' }));
    await waitFor(() => {
      expect(callMock.mock.calls.some((c) => c[0] === 'generate_mindmap')).toBe(true);
    });
    expect(await screen.findByRole('button', { name: /环境在替你做决定/ })).toBeTruthy();
  });

  it('点击概要节点打开证据抽屉，不把划线铺成叶子', async () => {
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: true, providerOff: false, cardCount: 4, cached: map, stale: false };
      }
      throw new Error(`未处理 ${cmd}`);
    });
    const opened: number[] = [];
    render(
      <MindmapView
        book={{ id: 1, title: '原子习惯' }}
        cards={[card(8, '环境是无形的手。'), card(9, '把书放在沙发上。')]}
        onToast={() => {}}
        onExit={() => {}}
        onOpenCard={(c) => opened.push(c.id)}
        onNeedSettings={() => {}}
      />,
    );
    const chip = await screen.findByRole('button', { name: /环境在替你做决定/ });
    expect(screen.queryByText('环境是无形的手。')).toBeNull();
    fireEvent.click(chip);
    expect(await screen.findByText('环境是无形的手。')).toBeTruthy();
    fireEvent.click(screen.getByText('环境是无形的手。'));
    expect(opened).toEqual([8]);
  });
});
