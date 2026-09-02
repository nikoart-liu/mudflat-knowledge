// 卡片墙滚动位置测试：打开新的书 / 回到总索引时，主栏滚动条应回到墙顶；
// 从设置往返到同一面墙时，位置保留（那是同一本书的阅读痕迹）。
// jsdom 没有布局，scrollTop 赋值会被钳到 0，因此用属性侦测器记录真实赋值。
import { describe, expect, it, vi, beforeEach, afterEach, beforeAll, type Mock } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import App from './App';
import { call, type BookRow, type CardRow } from './types';

// vitest globals:false 下 RTL 无法自动注册 cleanup，必须显式调用
afterEach(() => cleanup());

vi.mock('./types', async () => {
  const actual = await vi.importActual<typeof import('./types')>('./types');
  return { ...actual, call: vi.fn() };
});

const callMock = call as unknown as Mock;

const NOW = 1_700_000_000;

const books: BookRow[] = [
  {
    id: 1, wereadBookId: 'w1', title: '置身事内', author: '兰小欢', cover: '',
    readingProgress: 0, noteCount: 3, reviewCount: 0, syncReviews: true, syncedAt: null,
  },
  {
    id: 2, wereadBookId: 'w2', title: '夜晚的潜水艇', author: '陈春成', cover: '',
    readingProgress: 0, noteCount: 2, reviewCount: 0, syncReviews: true, syncedAt: null,
  },
];

function card(id: number, bookId: number, bookTitle: string, text: string): CardRow {
  return {
    id,
    kind: 'highlight',
    bookId,
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
    bookTitle,
    tags: [],
  };
}

const cardsByBook: Record<number, CardRow[]> = {
  1: [1, 2, 3].map((n) => card(n, 1, '置身事内', `第一本书划线${n}`)),
  2: [4, 5].map((n) => card(n, 2, '夜晚的潜水艇', `第二本书划线${n}`)),
};
const allCards = [...cardsByBook[1], ...cardsByBook[2]];

beforeAll(() => {
  // App 的 content-header 观察者依赖 ResizeObserver，jsdom 没有
  window.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

beforeEach(() => {
  callMock.mockReset();
  callMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case 'get_setup_status':
        return { hasKey: true, hasBooks: true };
      case 'list_books':
        return books;
      case 'list_tags':
        return [];
      case 'get_due_count':
        return 0;
      case 'query_cards': {
        const bookId = args?.filter ? (args.filter as { bookId?: number | null }).bookId : null;
        return bookId == null ? allCards : cardsByBook[bookId] ?? [];
      }
      case 'count_cards': {
        const bookId = args?.filter ? (args.filter as { bookId?: number | null }).bookId : null;
        return bookId == null ? allCards.length : cardsByBook[bookId]?.length ?? 0;
      }
      case 'get_settings':
        return { lastFullSync: null, dataDir: null };
      case 'get_review_settings':
        return { batchSize: 20 };
      case 'get_llm_settings':
        return { provider: 'off', baseUrl: '', model: '', hasKey: false };
      case 'get_mindmap_status':
        return { available: false, providerOff: true, cardCount: 0, cached: null, stale: false };
      default:
        throw new Error(`测试未处理的命令: ${cmd}`);
    }
  });
});

function spyScrollTop(el: HTMLElement) {
  let value = 0;
  Object.defineProperty(el, 'scrollTop', {
    configurable: true,
    get: () => value,
    set: (v: number) => { value = v; },
  });
  return {
    get: () => value,
    set: (v: number) => { value = v; },
  };
}

async function renderApp() {
  const utils = render(<App />);
  // 书架载入完成（第二本书的侧栏按钮出现）
  await screen.findByRole('button', { name: /夜晚的潜水艇/ });
  const main = utils.container.querySelector<HTMLElement>('.main');
  if (!main) throw new Error('.main 未渲染');
  return { ...utils, main };
}

describe('卡片墙滚动位置', () => {
  it('打开新的书时滚动条回到墙顶', async () => {
    const { main } = await renderApp();
    const scroll = spyScrollTop(main);
    scroll.set(400); // 模拟在总索引里读到了深处

    fireEvent.click(screen.getByRole('button', { name: /夜晚的潜水艇/ }));
    await screen.findByText('第二本书划线4'); // 新书的墙已渲染

    expect(scroll.get()).toBe(0);
  });

  it('回到总索引时滚动条也归零', async () => {
    const { main } = await renderApp();
    fireEvent.click(screen.getByRole('button', { name: /置身事内/ }));
    await screen.findByText('第一本书划线1');

    const scroll = spyScrollTop(main);
    scroll.set(300);
    fireEvent.click(screen.getByRole('button', { name: /全部索引/ }));
    await screen.findByText('第二本书划线5'); // 总索引的墙已渲染

    expect(scroll.get()).toBe(0);
  });

  it('设置往返回到同一面墙时保留滚动位置', async () => {
    const { main } = await renderApp();
    const scroll = spyScrollTop(main);
    scroll.set(300);

    fireEvent.click(screen.getByRole('button', { name: '设置' }));
    await screen.findByRole('heading', { name: '设置' });
    fireEvent.click(screen.getByRole('button', { name: '设置' }));
    await screen.findByRole('heading', { name: '全部索引' });

    expect(scroll.get()).toBe(300);
  });
});
