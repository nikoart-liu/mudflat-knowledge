// 书内划线墙章节目次：章节头「章节 N」打开篇目，点一章滚到分隔行。
import { describe, expect, it, vi, beforeEach, afterEach, beforeAll, type Mock } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import App from './App';
import { call, emptyLlmSettings, type BookRow, type CardRow } from './types';

const nativeScrollIntoView = HTMLElement.prototype.scrollIntoView;
afterEach(() => {
  HTMLElement.prototype.scrollIntoView = nativeScrollIntoView;
  cleanup();
});

vi.mock('./types', async () => {
  const actual = await vi.importActual<typeof import('./types')>('./types');
  return { ...actual, call: vi.fn() };
});

const callMock = call as unknown as Mock;
const NOW = 1_700_000_000;

const books: BookRow[] = [
  {
    id: 1, wereadBookId: 'w1', title: '置身事内', author: '兰小欢', cover: '',
    readingProgress: 0, noteCount: 4, reviewCount: 0, syncReviews: true, syncedAt: null,
    pinned: false, category: '', recentCardAt: null,
  },
];

function card(p: Partial<CardRow> & { id: number; text: string }): CardRow {
  return {
    kind: 'highlight',
    bookId: 1,
    remoteId: `r-${p.id}`,
    chapterUid: null,
    chapterTitle: null,
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
    ...p,
  };
}

const bookCards: CardRow[] = [
  card({ id: 1, text: '最近划的第三章', chapterUid: 3, chapterTitle: '第三章 地方政府的公司化' }),
  card({ id: 2, text: '第一章划线', chapterUid: 1, chapterTitle: '第一章' }),
  card({ id: 3, text: '未分章划线' }),
  card({ id: 4, text: '第三章另一条', chapterUid: 3, chapterTitle: '第三章 地方政府的公司化' }),
];

let cardTotal = bookCards.length;

beforeAll(() => {
  window.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

beforeEach(() => {
  callMock.mockReset();
  cardTotal = bookCards.length;
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
        return bookId === 1 ? bookCards : bookCards;
      }
      case 'count_cards':
        return cardTotal;
      case 'get_settings':
        return { lastFullSync: null, dataDir: null };
      case 'get_review_settings':
        return { batchSize: 20 };
      case 'get_llm_settings':
        return emptyLlmSettings();
      case 'get_mindmap_status':
        return { available: false, providerOff: true, cardCount: 0, cached: null, stale: false, chatEndpoint: null, model: null };
      case 'check_for_update':
        return { currentVersion: '0.1.0', latestVersion: '0.1.0', available: false, notes: '', htmlUrl: '', assetName: null, assetUrl: null };
      default:
        throw new Error(`测试未处理的命令: ${cmd}`);
    }
  });
});

async function openBook() {
  render(<App />);
  fireEvent.click(await screen.findByRole('button', { name: /置身事内/ }));
  await screen.findByText('最近划的第三章');
}

describe('书内章节目次', () => {
  it('载全后点「章节 N」打开目次，章序是阅读顺序而不是墙上的最近划过', async () => {
    await openBook();

    const chip = screen.getByRole('button', { name: '章节 2' });
    expect(chip.getAttribute('aria-expanded')).toBe('false');
    fireEvent.click(chip);

    expect(chip.getAttribute('aria-expanded')).toBe('true');
    const items = screen.getAllByRole('menuitem');
    expect(items.map((el) => el.getAttribute('aria-label'))).toEqual([
      '第一章，1 张',
      '第三章 地方政府的公司化，2 张',
      '未分章，1 张',
    ]);
  });

  it('点一章收起目次并滚到该章分隔行', async () => {
    const scrolled: HTMLElement[] = [];
    HTMLElement.prototype.scrollIntoView = function (this: HTMLElement) {
      scrolled.push(this);
    };
    await openBook();
    fireEvent.click(screen.getByRole('button', { name: '章节 2' }));
    fireEvent.click(screen.getByRole('menuitem', { name: '第一章，1 张' }));

    expect(screen.queryByRole('menu')).toBeNull();
    const head = document.querySelector('[data-group-key="c-1"]');
    expect(scrolled[0]).toBe(head);
    expect(document.activeElement).toBe(head);
  });

  it('墙未载全时不出章节入口', async () => {
    cardTotal = bookCards.length + 1;
    await openBook();
    expect(screen.queryByRole('button', { name: /章节/ })).toBeNull();
  });

  it('总索引不出章节入口', async () => {
    render(<App />);
    await screen.findByText('最近划的第三章');
    expect(screen.queryByRole('button', { name: /章节/ })).toBeNull();
  });

  it('Esc 与点外面收起目次', async () => {
    await openBook();
    const chip = screen.getByRole('button', { name: '章节 2' });
    fireEvent.click(chip);
    expect(screen.getByRole('menu')).toBeTruthy();

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('menu')).toBeNull();
    expect(chip.getAttribute('aria-expanded')).toBe('false');

    fireEvent.click(chip);
    expect(screen.getByRole('menu')).toBeTruthy();
    fireEvent.pointerDown(document.body);
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('打开后方向键在章之间移动', async () => {
    await openBook();
    fireEvent.click(screen.getByRole('button', { name: '章节 2' }));
    const items = screen.getAllByRole('menuitem');
    expect(document.activeElement).toBe(items[0]);
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(items[1]);
    fireEvent.keyDown(window, { key: 'ArrowUp' });
    expect(document.activeElement).toBe(items[0]);
  });
});
