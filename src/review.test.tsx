// 回顾页交互测试（PRD 12.1 / R1、R2、R3 验收）：
// - 翻面前不出现评分栏，数字键不提交；
// - 空格只翻面；翻面后 1–4 快捷键与点击提交对应档位并前进；
// - 评分失败停留当前卡、进度不递增、可重试；
// - 结算分支：真实到期数为 0 → 「今天翻完了」；>0 → 「本批完成，还剩 N」+ 继续下一批；
// - 首次「移出回顾」先确认说明，之后直接执行。
import { describe, expect, it, vi, beforeEach, afterEach, type Mock } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { ReviewView } from './App';
import { call, type CardRow } from './types';

// vitest globals:false 下 RTL 无法自动注册 cleanup，必须显式调用
afterEach(() => cleanup());

vi.mock('./types', async () => {
  const actual = await vi.importActual<typeof import('./types')>('./types');
  return { ...actual, call: vi.fn() };
});

const callMock = call as unknown as Mock;

const NOW = 1_700_000_000;

function card(id: number, text: string): CardRow {
  return {
    id,
    kind: 'highlight',
    bookId: 1,
    remoteId: `r-${id}`,
    chapterUid: 1,
    chapterTitle: '第一章',
    text,
    abstractText: null,
    rangeStr: null,
    note: '',
    starred: false,
    excludedFromReview: false,
    createdAt: NOW,
    updatedAt: NOW,
    deleted: false,
    bookTitle: '置身事内',
    tags: [],
  };
}

const backend = {
  dueCount: 0,
  batchSize: 20,
  queue: [] as CardRow[],
  graded: [] as { cardId: number; rating: string }[],
  excluded: [] as { id: number; excluded: boolean }[],
  restored: [] as { cardId: number }[],
  failGrading: false,
};

function mockBackend() {
  callMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case 'get_due_count':
        return backend.dueCount;
      case 'get_review_settings':
        return { batchSize: backend.batchSize };
      case 'get_due_cards':
        return backend.queue.slice(0, Number(args?.limit ?? 30));
      case 'grade_review': {
        if (backend.failGrading) throw new Error('评分失败');
        backend.graded.push({ cardId: Number(args?.cardId), rating: String(args?.rating) });
        return {
          prev: { due_at: 0, interval_days: 0, ease: 2.5, reps: 0, lapses: 0 },
          next: { due_at: NOW + 86_400, interval_days: 1, ease: 2.5, reps: 1, lapses: 0 },
        };
      }
      case 'restore_review_state':
        backend.restored.push({ cardId: Number(args?.cardId) });
        return undefined;
      case 'set_excluded_from_review':
        backend.excluded.push({ id: Number(args?.id), excluded: Boolean(args?.excluded) });
        return undefined;
      default:
        throw new Error(`测试未处理的命令: ${cmd}`);
    }
  });
}

function backNum(container: HTMLElement): string {
  const el = container.querySelector('.deck-back-num');
  if (!el) return '';
  return (el.textContent ?? '').split('/')[0].trim();
}

async function startReview(container: HTMLElement) {
  fireEvent.click(await screen.findByRole('button', { name: /开始翻牌/ }));
  await waitFor(() => expect(backNum(container)).toBe('01'));
}

beforeEach(() => {
  localStorage.clear();
  backend.dueCount = 0;
  backend.batchSize = 20;
  backend.queue = [];
  backend.graded = [];
  backend.excluded = [];
  backend.restored = [];
  backend.failGrading = false;
  callMock.mockReset();
  mockBackend();
});

describe('ReviewView 四档评分（R1）', () => {
  it('翻面前无评分栏，数字键不提交', async () => {
    backend.dueCount = 2;
    backend.queue = [card(1, '第一条'), card(2, '第二条')];
    const { container } = render(
      <ReviewView onToast={vi.fn()} onExit={vi.fn()} />,
    );
    await startReview(container);

    expect(screen.queryByRole('group', { name: '记忆评分' })).toBeNull();
    fireEvent.keyDown(window, { key: '1' });
    fireEvent.keyDown(window, { key: '3' });
    await waitFor(() => expect(callMock).toHaveBeenCalled());
    expect(backend.graded).toEqual([]);
    expect(backNum(container)).toBe('01');
  });

  it('空格只翻面不评分；翻面后 1–4 提交对应档位并前进', async () => {
    backend.dueCount = 2;
    backend.queue = [card(1, '第一条'), card(2, '第二条')];
    const { container } = render(
      <ReviewView onToast={vi.fn()} onExit={vi.fn()} />,
    );
    await startReview(container);

    // 空格：翻面
    fireEvent.keyDown(window, { key: ' ' });
    const bar = await screen.findByRole('group', { name: '记忆评分' });
    expect(within(bar).getByText('忘了')).toBeTruthy();
    expect(within(bar).getByText('困难')).toBeTruthy();
    expect(within(bar).getByText('记得')).toBeTruthy();
    expect(within(bar).getByText('简单')).toBeTruthy();
    // 已翻面后空格不再触发任何提交（R1.2：不再自动提交 Good）
    fireEvent.keyDown(window, { key: ' ' });
    expect(backend.graded).toEqual([]);

    // 数字键 2 = 困难
    fireEvent.keyDown(window, { key: '2' });
    await waitFor(() =>
      expect(backend.graded).toEqual([{ cardId: 1, rating: 'hard' }]),
    );
    // 飞出动画后进入第二张（背面朝上，需先翻面）
    await waitFor(() => expect(backNum(container)).toBe('02'), { timeout: 2000 });
    fireEvent.keyDown(window, { key: ' ' });
    await screen.findByRole('group', { name: '记忆评分' });

    // 数字键 4 = 简单
    fireEvent.keyDown(window, { key: '4' });
    await waitFor(() =>
      expect(backend.graded).toEqual([
        { cardId: 1, rating: 'hard' },
        { cardId: 2, rating: 'easy' },
      ]),
    );
  });

  it('评分失败停留当前卡，进度不递增，可重试', async () => {
    backend.dueCount = 2;
    backend.queue = [card(1, '第一条'), card(2, '第二条')];
    backend.failGrading = true;
    const onToast = vi.fn();
    const { container } = render(
      <ReviewView onToast={onToast} onExit={vi.fn()} />,
    );
    await startReview(container);

    fireEvent.keyDown(window, { key: ' ' });
    await screen.findByRole('group', { name: '记忆评分' });
    fireEvent.keyDown(window, { key: '3' });

    await waitFor(() => expect(onToast).toHaveBeenCalled());
    expect(String(onToast.mock.calls[0][0])).toContain('评分失败');
    expect(backNum(container)).toBe('01');
    expect(backend.graded).toEqual([]);

    // 恢复后可重试成功
    backend.failGrading = false;
    await waitFor(() =>
      expect(
        (screen.getByRole('button', { name: /记得/ }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    fireEvent.keyDown(window, { key: '3' });
    await waitFor(() =>
      expect(backend.graded).toEqual([{ cardId: 1, rating: 'good' }]),
    );
    await waitFor(() => expect(backNum(container)).toBe('02'), { timeout: 2000 });
  });

  it('评分后 Z 撤销上一张，回到该卡正面', async () => {
    backend.dueCount = 2;
    backend.queue = [card(1, '第一条'), card(2, '第二条')];
    const { container } = render(
      <ReviewView onToast={vi.fn()} onExit={vi.fn()} />,
    );
    await startReview(container);
    fireEvent.keyDown(window, { key: ' ' });
    await screen.findByRole('group', { name: '记忆评分' });
    fireEvent.keyDown(window, { key: '3' });
    await waitFor(() => expect(backNum(container)).toBe('02'), { timeout: 2000 });

    fireEvent.keyDown(window, { key: 'z' });
    await waitFor(() => expect(backend.restored).toEqual([{ cardId: 1 }]));
    await waitFor(() => expect(backNum(container)).toBe('01'));
    expect(screen.getByRole('group', { name: '记忆评分' })).toBeTruthy();
  });
});

describe('ReviewView 结算分支（R3）', () => {
  it('到期 0：显示「今天翻完了」', async () => {
    backend.dueCount = 1;
    backend.queue = [card(1, '唯一一张')];
    const { container } = render(
      <ReviewView onToast={vi.fn()} onExit={vi.fn()} />,
    );
    await startReview(container);
    backend.dueCount = 0; // 结算时重查为 0
    fireEvent.keyDown(window, { key: ' ' });
    await screen.findByRole('group', { name: '记忆评分' });
    fireEvent.keyDown(window, { key: '3' });

    expect(await screen.findByText('今天翻完了')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /继续下一批/ })).toBeNull();
  });

  it('到期 25：显示「本批完成，今天还剩 25 张」，可继续下一批', async () => {
    backend.dueCount = 45;
    backend.queue = [card(1, '第一批之一'), card(2, '第一批之二')];
    const { container } = render(
      <ReviewView onToast={vi.fn()} onExit={vi.fn()} />,
    );
    await startReview(container);
    backend.dueCount = 25; // 首批结束后还剩 25
    fireEvent.keyDown(window, { key: ' ' });
    await screen.findByRole('group', { name: '记忆评分' });
    fireEvent.keyDown(window, { key: '3' });
    await waitFor(() => expect(backNum(container)).toBe('02'), { timeout: 2000 });
    // 第二张同样先翻面再评分
    fireEvent.keyDown(window, { key: ' ' });
    await screen.findByRole('group', { name: '记忆评分' });
    fireEvent.keyDown(window, { key: '3' });

    expect(await screen.findByText('本批完成，今天还剩 25 张')).toBeTruthy();
    expect(screen.getByText(/本批处理 2 张/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /继续下一批/ }));

    backend.queue = [card(3, '第二批之一')];
    await waitFor(() => expect(backNum(container)).toBe('01'), { timeout: 2000 });
    expect(container.querySelector('.deck-front-inner')?.textContent).toContain('第二批之一');
  });
});

describe('ReviewView 移出回顾（R2）', () => {
  it('首次先确认说明；确认后排除并进入下一张；再次操作不再询问', async () => {
    backend.dueCount = 3;
    backend.queue = [card(1, '低价值'), card(2, '也移出'), card(3, '留下的')];
    const { container } = render(
      <ReviewView onToast={vi.fn()} onExit={vi.fn()} />,
    );
    await startReview(container);

    fireEvent.keyDown(window, { key: ' ' });
    await screen.findByRole('group', { name: '记忆评分' });
    fireEvent.click(screen.getByRole('button', { name: '移出回顾' }));

    // 首次：出现说明弹层
    const dialog = await screen.findByRole('dialog');
    expect(dialog.textContent).toContain('仍会保留在卡片墙与搜索中');
    fireEvent.click(within(dialog).getByRole('button', { name: '移出回顾' }));

    await waitFor(() =>
      expect(backend.excluded).toEqual([{ id: 1, excluded: true }]),
    );
    await waitFor(() => expect(backNum(container)).toBe('02'), { timeout: 2000 });

    // 第二次：不再询问，直接排除
    fireEvent.keyDown(window, { key: ' ' });
    await screen.findByRole('group', { name: '记忆评分' });
    fireEvent.click(screen.getByRole('button', { name: '移出回顾' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    await waitFor(() =>
      expect(backend.excluded).toEqual([
        { id: 1, excluded: true },
        { id: 2, excluded: true },
      ]),
    );
  });

  it('移出后 Z 撤销，卡片回到队列', async () => {
    backend.dueCount = 2;
    backend.queue = [card(1, '低价值'), card(2, '留下的')];
    const { container } = render(
      <ReviewView onToast={vi.fn()} onExit={vi.fn()} />,
    );
    await startReview(container);
    fireEvent.keyDown(window, { key: ' ' });
    await screen.findByRole('group', { name: '记忆评分' });
    fireEvent.click(screen.getByRole('button', { name: '移出回顾' }));
    fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: '移出回顾' }));
    await waitFor(() => expect(backNum(container)).toBe('02'), { timeout: 2000 });

    fireEvent.keyDown(window, { key: 'z' });
    await waitFor(() =>
      expect(backend.excluded).toEqual([
        { id: 1, excluded: true },
        { id: 1, excluded: false },
      ]),
    );
    await waitFor(() => expect(backNum(container)).toBe('01'));
  });
});

describe('ReviewView 本书清样', () => {
  it('到期查询与取卡都带 bookId，文案与范围行指向本书', async () => {
    backend.dueCount = 2;
    backend.queue = [card(1, '本书卡')];
    const { container } = render(
      <ReviewView book={{ id: 7, title: '置身事内' }} onToast={vi.fn()} onExit={vi.fn()} />,
    );

    expect(await screen.findByText(/本书到期 2 张/)).toBeTruthy();
    expect(callMock).toHaveBeenCalledWith('get_due_count', { bookId: 7 });

    await startReview(container);
    expect(callMock).toHaveBeenCalledWith('get_due_cards', { limit: 20, bookId: 7 });
    expect(screen.getAllByText('本书 · 置身事内').length).toBeGreaterThan(0);

    // 结算重查也按书范围
    fireEvent.keyDown(window, { key: ' ' });
    await screen.findByRole('group', { name: '记忆评分' });
    fireEvent.keyDown(window, { key: '3' });
    expect(await screen.findByText(/本批完成/)).toBeTruthy();
    expect(callMock).toHaveBeenCalledWith('get_due_count', { bookId: 7 });
  });

  it('本书到期为 0：文案指向这本书而非全馆', async () => {
    backend.dueCount = 0;
    render(
      <ReviewView book={{ id: 7, title: '置身事内' }} onToast={vi.fn()} onExit={vi.fn()} />,
    );
    expect(await screen.findByText('这本书当前没有到期卡片')).toBeTruthy();
    expect(screen.queryByText('当前没有到期卡片')).toBeNull();
  });
});
