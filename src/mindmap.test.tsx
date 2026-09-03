import { describe, expect, it, vi, beforeEach, afterEach, type Mock } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MindmapView } from './App';
import { call, type CardRow, type Mindmap, type MindmapEventPayload } from './types';

afterEach(() => cleanup());

vi.mock('@tauri-apps/api/core', () => {
  class Channel {
    onmessage: ((ev: unknown) => void) | undefined;
  }
  return { Channel, invoke: vi.fn() };
});

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

const branched: Mindmap = {
  ...map,
  stats: { ...map.stats, themes: 2 },
  root: {
    ...map.root,
    children: [
      {
        id: 't-env',
        label: '环境在替你做决定',
        kind: 'theme',
        summary: '少靠自控，多改摆设。',
        sourceCardIds: [8, 9],
        children: [
          { id: 't-env-1', label: '把充电器换房间', kind: 'theme', sourceCardIds: [8], children: [] },
          { id: 't-env-2', label: '书放在沙发上', kind: 'theme', sourceCardIds: [9], children: [] },
        ],
      },
      {
        id: 't-id',
        label: '身份由重复塑造',
        kind: 'theme',
        sourceCardIds: [8],
        children: [
          { id: 't-id-1', label: '先成为那种人', kind: 'theme', sourceCardIds: [8], children: [] },
        ],
      },
    ],
  },
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

  it('归纳中展示当前步骤、分数进度和章失败原因', async () => {
    let finish!: (value: Mindmap) => void;
    const pending = new Promise<Mindmap>((resolve) => { finish = resolve; });
    type ProgressChan = { onmessage?: (ev: MindmapEventPayload) => void };
    callMock.mockImplementation(async (cmd?: string, opts?: { onProgress?: ProgressChan }) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return {
          available: true, providerOff: false, cardCount: 186, cached: null, stale: false,
          chatEndpoint: 'https://api.deepseek.com/v1/chat/completions', model: 'deepseek-chat',
        };
      }
      if (cmd === 'generate_mindmap') {
        const send = opts?.onProgress?.onmessage;
        send?.({ stage: 'start', current: 0, total: 8, title: '', message: '186 张卡片，按 8 章归纳' });
        send?.({ stage: 'chapter', current: 2, total: 8, title: '环境', message: '' });
        send?.({ stage: 'chapter_failed', current: 3, total: 8, title: '坏习惯', message: '连接被对端断开' });
        return pending;
      }
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
    await waitFor(() => expect(screen.getByRole('button', { name: '生成线索' })).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: '生成线索' }));
    const status = await screen.findByRole('status');
    expect(status.textContent).toContain('「坏习惯」失败：连接被对端断开');
    expect(screen.getByText('3 / 8')).toBeTruthy();
    expect((screen.getByRole('button', { name: '归纳中…' }) as HTMLButtonElement).disabled).toBe(true);
    finish(map);
    expect(await screen.findByRole('button', { name: /环境在替你做决定/ })).toBeTruthy();
  });

  it('整次失败后仍列出已经失败的章，按钮恢复可点', async () => {
    type ProgressChan = { onmessage?: (ev: MindmapEventPayload) => void };
    callMock.mockImplementation(async (cmd?: string, opts?: { onProgress?: ProgressChan }) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return {
          available: true, providerOff: false, cardCount: 186, cached: null, stale: false,
          chatEndpoint: 'https://api.deepseek.com/v1/chat/completions', model: 'deepseek-chat',
        };
      }
      if (cmd === 'generate_mindmap') {
        opts?.onProgress?.onmessage?.({
          stage: 'chapter_failed', current: 1, total: 5, title: '坏习惯', message: '连接被对端断开',
        });
        throw new Error('没有得到可用的概要节点。\n丢掉「坏习惯」：只有 1 张证据，至少要 2 张\n请重试，或换一个模型。');
      }
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
    await waitFor(() => expect(screen.getByRole('button', { name: '生成线索' })).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: '生成线索' }));
    expect(await screen.findByText(/丢掉「坏习惯」：只有 1 张证据，至少要 2 张/)).toBeTruthy();
    expect(screen.getByText('「坏习惯」失败：连接被对端断开')).toBeTruthy();
    expect((screen.getByRole('button', { name: '生成线索' }) as HTMLButtonElement).disabled).toBe(false);
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
    expect(screen.queryByRole('button', { name: /子题/ })).toBeNull();
  });

  it('有二级的一级露出子题 N，默认不画二级', async () => {
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: true, providerOff: false, cardCount: 4, cached: branched, stale: false };
      }
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
    expect(await screen.findByRole('button', { name: '子题 2' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '子题 1' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /把充电器换房间/ })).toBeNull();
    expect(screen.queryByRole('button', { name: /先成为那种人/ })).toBeNull();
  });

  it('点子题 N 只长出该枝，不开证据抽屉', async () => {
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: true, providerOff: false, cardCount: 4, cached: branched, stale: false };
      }
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
    fireEvent.click(await screen.findByRole('button', { name: '子题 2' }));
    expect(await screen.findByRole('button', { name: /把充电器换房间/ })).toBeTruthy();
    expect(screen.getByRole('button', { name: /书放在沙发上/ })).toBeTruthy();
    expect(screen.queryByText('环境是无形的手。')).toBeNull();
    expect(screen.queryByRole('button', { name: /先成为那种人/ })).toBeNull();
  });

  it('点有子题的一级标题仍开证据抽屉', async () => {
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: true, providerOff: false, cardCount: 4, cached: branched, stale: false };
      }
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
    fireEvent.click(await screen.findByRole('button', { name: /环境在替你做决定/ }));
    expect(await screen.findByText('环境是无形的手。')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /把充电器换房间/ })).toBeNull();
  });

  it('展开另一枝时收回上一枝', async () => {
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: true, providerOff: false, cardCount: 4, cached: branched, stale: false };
      }
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
    fireEvent.click(await screen.findByRole('button', { name: '子题 2' }));
    expect(await screen.findByRole('button', { name: /把充电器换房间/ })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '子题 1' }));
    expect(await screen.findByRole('button', { name: /先成为那种人/ })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /把充电器换房间/ })).toBeNull();
  });

  it('证据抽屉能展示卡片用户笔记与想法原文', async () => {
    const cardWithNote: CardRow = {
      ...card(8, '环境是无形的手。'),
      note: '这是我自己的批注',
      tags: ['心理学', '习惯'],
    };
    const cardThought: CardRow = {
      ...card(9, '我的感悟'),
      kind: 'thought',
      abstractText: '书中的原文引用',
    };
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: true, providerOff: false, cardCount: 2, cached: map, stale: false };
      }
      throw new Error(`未处理 ${cmd}`);
    });
    render(
      <MindmapView
        book={{ id: 1, title: '原子习惯' }}
        cards={[cardWithNote, cardThought]}
        onToast={() => {}}
        onExit={() => {}}
        onOpenCard={() => {}}
        onNeedSettings={() => {}}
      />,
    );
    fireEvent.click(await screen.findByRole('button', { name: /环境在替你做决定/ }));
    expect(await screen.findByText('这是我自己的批注')).toBeTruthy();
    expect(screen.getByText('#心理学')).toBeTruthy();
    expect(screen.getByText('书中的原文引用')).toBeTruthy();
  });

  it('独立通过 query_cards 查询全书卡片，弥补外部 cards 缺失', async () => {
    const remoteCard8 = card(8, '从后端独立拉取的划线证据');
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: true, providerOff: false, cardCount: 2, cached: map, stale: false };
      }
      if (cmd === 'query_cards') {
        return [remoteCard8];
      }
      throw new Error(`未处理 ${cmd}`);
    });
    render(
      <MindmapView
        book={{ id: 1, title: '原子习惯' }}
        cards={[]} // 外部传入空数组（例如卡片墙过滤导致空）
        onToast={() => {}}
        onExit={() => {}}
        onOpenCard={() => {}}
        onNeedSettings={() => {}}
      />,
    );
    fireEvent.click(await screen.findByRole('button', { name: /环境在替你做决定/ }));
    expect(await screen.findByText('从后端独立拉取的划线证据')).toBeTruthy();
  });

  it('支持复制大纲与缩放居中控制栏', async () => {
    const toasts: string[] = [];
    const writeTextMock = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText: writeTextMock } });

    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: true, providerOff: false, cardCount: 4, cached: map, stale: false };
      }
      throw new Error(`未处理 ${cmd}`);
    });
    render(
      <MindmapView
        book={{ id: 1, title: '原子习惯' }}
        cards={[card(8, '环境是无形的手。'), card(9, '把书放在沙发上。')]}
        onToast={(m) => toasts.push(m)}
        onExit={() => {}}
        onOpenCard={() => {}}
        onNeedSettings={() => {}}
      />,
    );
    expect(await screen.findByRole('button', { name: '复制大纲' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '居中' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '100%' })).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: '复制大纲' }));
    await waitFor(() => expect(writeTextMock).toHaveBeenCalled());
    expect(writeTextMock.mock.calls[0][0]).toContain('# 线索 · 原子习惯');
    expect(toasts).toContain('已复制线索大纲到剪贴板');
  });

  it('非失效状态下点重新生成触发二次确认弹层', async () => {
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: true, providerOff: false, cardCount: 4, cached: map, stale: false };
      }
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
    fireEvent.click(await screen.findByRole('button', { name: '重新生成' }));
    expect(await screen.findByText('重新归纳线索')).toBeTruthy();
    expect(screen.getByText('确定要重新归纳吗？当前已生成的线索结构将被新结果覆盖。')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    expect(screen.queryByText('重新归纳线索')).toBeNull();
  });

  it('Esc 键分级响应：优先关抽屉，其次收起子题，最后才退出', async () => {
    const exited: string[] = [];
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: true, providerOff: false, cardCount: 4, cached: branched, stale: false };
      }
      throw new Error(`未处理 ${cmd}`);
    });
    render(
      <MindmapView
        book={{ id: 1, title: '原子习惯' }}
        cards={[card(8, '环境是无形的手。'), card(9, '把书放在沙发上。')]}
        onToast={() => {}}
        onExit={() => exited.push('exit')}
        onOpenCard={() => {}}
        onNeedSettings={() => {}}
      />,
    );
    // 1. 展开子题
    fireEvent.click(await screen.findByRole('button', { name: '子题 2' }));
    expect(await screen.findByRole('button', { name: /把充电器换房间/ })).toBeTruthy();

    // 2. 打开抽屉
    fireEvent.click(screen.getByRole('button', { name: /把充电器换房间/ }));
    expect(await screen.findByRole('button', { name: '复制本题' })).toBeTruthy();

    // 3. 第 1 次 Esc：关闭抽屉，子题仍然展开
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('button', { name: '复制本题' })).toBeNull();
    expect(screen.getByRole('button', { name: /把充电器换房间/ })).toBeTruthy();
    expect(exited).toHaveLength(0);

    // 4. 第 2 次 Esc：收起子题分支
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('button', { name: /把充电器换房间/ })).toBeNull();
    expect(exited).toHaveLength(0);

    // 5. 第 3 次 Esc：退出线索界面
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(exited).toEqual(['exit']);
  });

  it('放大、缩小、居中与证据抽屉均能正常点击交互', async () => {
    const openedCards: number[] = [];
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return { available: true, providerOff: false, cardCount: 4, cached: map, stale: false };
      }
      throw new Error(`未处理 ${cmd}`);
    });
    render(
      <MindmapView
        book={{ id: 1, title: '原子习惯' }}
        cards={[card(8, '环境是无形的手。'), card(9, '把书放在沙发上。')]}
        onToast={() => {}}
        onExit={() => {}}
        onOpenCard={(c) => openedCards.push(c.id)}
        onNeedSettings={() => {}}
      />,
    );

    // 1. 验证缩放按钮点击
    expect(await screen.findByRole('button', { name: '100%' })).toBeTruthy();
    const btnPlus = screen.getByRole('button', { name: '+' });
    const btnMinus = screen.getByRole('button', { name: '-' });
    const btnCenter = screen.getByRole('button', { name: '居中' });

    fireEvent.click(btnPlus);
    expect(screen.getByRole('button', { name: '115%' })).toBeTruthy();
    fireEvent.click(btnMinus);
    expect(screen.getByRole('button', { name: '100%' })).toBeTruthy();
    fireEvent.click(btnCenter);

    // 2. 验证点击主题节点打开证据抽屉
    const nodeChip = screen.getByRole('button', { name: /环境在替你做决定/ });
    fireEvent.click(nodeChip);

    // 3. 验证证据抽屉打开并渲染证据卡片
    expect(await screen.findByText('证据 2 张')).toBeTruthy();
    expect(screen.getByText('环境是无形的手。')).toBeTruthy();

    // 4. 验证点击抽屉内的证据卡片
    fireEvent.click(screen.getByText('环境是无形的手。'));
    expect(openedCards).toEqual([8]);

    // 5. 验证关闭抽屉
    fireEvent.click(screen.getAllByRole('button', { name: '关闭' })[0]);
    expect(screen.queryByText('证据 2 张')).toBeNull();
  });

  it('挂载时若已有进行中的后台任务，能够正确恢复进度条且不丢失任务状态', async () => {
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return {
          available: true,
          providerOff: false,
          cardCount: 4,
          cached: null,
          stale: false,
        };
      }
      throw new Error(`未处理 ${cmd}`);
    });
    const task = {
      bookId: 1,
      bookTitle: '原子习惯',
      busy: true,
      progress: '正在归纳「第三章」…',
      progressFrac: { current: 3, total: 5 },
      progressFails: [],
      elapsed: 18,
      error: null,
    };
    render(
      <MindmapView
        book={{ id: 1, title: '原子习惯' }}
        cards={[card(8, '环境是无形的手。'), card(9, '把书放在沙发上。')]}
        onToast={() => {}}
        onExit={() => {}}
        onOpenCard={() => {}}
        onNeedSettings={() => {}}
        activeTask={task}
      />,
    );

    // 验证进度与运行秒数正确恢复，没有丢失
    expect(await screen.findByText('正在归纳「第三章」…')).toBeTruthy();
    expect(screen.getByText('3 / 5')).toBeTruthy();
    expect(screen.getByText('已等待 18 秒')).toBeTruthy();
    const btn = screen.getByRole('button', { name: '归纳中…' });
    expect(btn).toBeTruthy();
    expect((btn as HTMLButtonElement).disabled).toBe(true);
  });

  it('委托全局 onStartGenerate 触发归纳任务，避免组件卸载时任务丢失', async () => {
    const startedBooks: string[] = [];
    callMock.mockImplementation(async (cmd?: string) => {
      if (!cmd || cmd === 'get_mindmap_status') {
        return {
          available: true,
          providerOff: false,
          cardCount: 4,
          cached: null,
          stale: false,
          chatEndpoint: 'https://api.openai.com/v1/chat/completions',
          model: 'gpt-4o',
        };
      }
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
        onStartGenerate={async (b) => {
          startedBooks.push(b.title);
        }}
      />,
    );
    fireEvent.click(await screen.findByRole('button', { name: '生成线索' }));
    expect(startedBooks).toEqual(['原子习惯']);
  });
});
