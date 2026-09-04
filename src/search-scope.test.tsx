// 检索范围切换：慢请求的纯原词结果不能盖掉后发出去的「意思相关」。
import { describe, expect, it, vi, beforeEach, afterEach, beforeAll, type Mock } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import App from './App';
import { call, emptyLlmSettings, type BookRow, type CardFilter, type CardRow, type SearchHit } from './types';

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
    readingProgress: 0, noteCount: 2, reviewCount: 0, syncReviews: true, syncedAt: null,
    pinned: false, category: '', recentCardAt: null,
  },
];

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

const lexical = card(1, '工作记忆容量有限。');
const semantic = card(2, '脑子里同时能握住的东西很少。');

function hit(c: CardRow, matchKind: SearchHit['matchKind']): SearchHit {
  return { card: c, matchKind };
}

beforeAll(() => {
  window.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

function delay(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

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
      case 'query_cards':
        return [lexical, semantic];
      case 'count_cards':
        return 2;
      case 'get_settings':
        return { lastFullSync: null, dataDir: null };
      case 'get_review_settings':
        return { batchSize: 20 };
      case 'get_llm_settings':
        return emptyLlmSettings();
      case 'get_mindmap_status':
        return { available: false, providerOff: true, cardCount: 0, cached: null, stale: false, chatEndpoint: null, model: null };
      case 'search_cards': {
        const filter = args?.filter as CardFilter | undefined;
        const scoped = filter?.bookId != null;
        if (scoped) {
          await delay(30);
          return [hit(lexical, 'lexical'), hit(semantic, 'semantic')];
        }
        await delay(400);
        return [hit(lexical, 'lexical')];
      }
      case 'check_for_update':
        return { currentVersion: '0.1.0', latestVersion: '0.1.0', available: false, notes: '', htmlUrl: '', assetName: null, assetUrl: null };
      default:
        throw new Error(`测试未处理的命令: ${cmd}`);
    }
  });
});

describe('检索范围切换', () => {
  it('慢的「搜全部」纯原词结果不能盖掉后到的意思相关', async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: /置身事内/ }));
    await screen.findByText('工作记忆容量有限。');

    fireEvent.change(screen.getByRole('searchbox'), { target: { value: '工作记忆' } });
    expect(await screen.findByText('意思相关')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: '搜索全部卡片' }));
    await delay(300);
    fireEvent.click(await screen.findByRole('button', { name: '只搜当前范围' }));

    await delay(800);
    expect(screen.getByText('意思相关')).toBeTruthy();
    expect(screen.getByText('脑子里同时能握住的东西很少。')).toBeTruthy();
  });
});
