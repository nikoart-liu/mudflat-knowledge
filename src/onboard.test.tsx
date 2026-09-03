// 创刊空态：无 Key 时在墙上签发并粘贴，不把人赶到设置页；
// 保存并同步后墙面被自己的书填满；检索/筛选空态给原因与退路。
import { describe, expect, it, vi, beforeEach, afterEach, beforeAll, type Mock } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import App from './App';
import { call, type BookRow, type CardRow } from './types';

afterEach(() => cleanup());

vi.mock('@tauri-apps/api/core', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@tauri-apps/api/core')>();
  return {
    ...actual,
    Channel: class Channel {
      onmessage: ((ev: unknown) => void) | null = null;
    },
  };
});

vi.mock('./types', async () => {
  const actual = await vi.importActual<typeof import('./types')>('./types');
  return { ...actual, call: vi.fn() };
});

const callMock = call as unknown as Mock;
const NOW = 1_700_000_000;

const book: BookRow = {
  id: 1, wereadBookId: 'w1', title: '置身事内', author: '兰小欢', cover: '',
  readingProgress: 0, noteCount: 1, reviewCount: 0, syncReviews: true, syncedAt: NOW,
};

const card: CardRow = {
  id: 1, kind: 'highlight', bookId: 1, remoteId: 'r-1', chapterUid: 1,
  chapterTitle: '第一章', text: '地方政府像一家公司。', abstractText: null, rangeStr: null,
  note: '', starred: false, excludedFromReview: false, createdAt: NOW, updatedAt: NOW,
  deleted: false, bookTitle: '置身事内', tags: [],
};

const backend = {
  hasKey: false,
  hasBooks: false,
  books: [] as BookRow[],
  cards: [] as CardRow[],
  saveError: null as string | null,
  synced: false,
};

function mockBackend() {
  callMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case 'get_setup_status':
        return { hasKey: backend.hasKey, hasBooks: backend.hasBooks };
      case 'list_books':
        return backend.books;
      case 'list_tags':
        return [];
      case 'get_due_count':
        return 0;
      case 'query_cards':
        return backend.cards;
      case 'count_cards':
        return backend.cards.length;
      case 'open_external':
        return null;
      case 'save_api_key': {
        if (backend.saveError) throw new Error(backend.saveError);
        backend.hasKey = true;
        return null;
      }
      case 'sync_all': {
        backend.synced = true;
        backend.hasBooks = true;
        backend.books = [book];
        backend.cards = [card];
        return { booksSynced: 1, booksFailed: 0, added: 1, removed: 0, failures: [] };
      }
      case 'get_settings':
        return { lastFullSync: null, dataDir: null };
      case 'get_review_settings':
        return { batchSize: 20 };
      case 'get_llm_settings':
        return {
          provider: 'off', baseUrl: '', model: '', hasKey: false,
          embeddingProvider: 'off', embeddingBaseUrl: '', embeddingModel: '', hasEmbeddingKey: false,
        };
      case 'get_mindmap_status':
        return { available: false, providerOff: true, cardCount: 0, cached: null, stale: false, chatEndpoint: null, model: null };
      default:
        throw new Error(`测试未处理的命令: ${cmd}`);
    }
  });
}

beforeAll(() => {
  window.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

beforeEach(() => {
  backend.hasKey = false;
  backend.hasBooks = false;
  backend.books = [];
  backend.cards = [];
  backend.saveError = null;
  backend.synced = false;
  callMock.mockReset();
  mockBackend();
});

async function renderEmpty() {
  render(<App />);
  expect(await screen.findByText('把微信读书的划线接到这面墙上')).toBeTruthy();
}

describe('创刊空态', () => {
  it('馆空无 Key 时在墙上写出发刊词、签发步骤和贴 Key 表单，不跳设置', async () => {
    await renderEmpty();
    expect(screen.getByText('创刊')).toBeTruthy();
    expect(screen.getByRole('heading', { name: '尚未接上' })).toBeTruthy();
    expect(screen.getByText(/开通 Skills、签发 Key/)).toBeTruthy();
    expect(screen.getByRole('button', { name: '去签发 Key' })).toBeTruthy();
    expect(screen.getByLabelText('微信读书 API Key')).toBeTruthy();
    expect(screen.getByRole('button', { name: '保存并同步' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '只保存' })).toBeTruthy();
    expect(screen.queryByRole('heading', { name: '设置' })).toBeNull();
    expect(screen.getByText('同步之后，有笔记的书会出现在这里。')).toBeTruthy();
  });

  it('去签发 Key 打开微信读书 Skills 开通页', async () => {
    await renderEmpty();
    fireEvent.click(screen.getByRole('button', { name: '去签发 Key' }));
    await waitFor(() => {
      const open = callMock.mock.calls.find((c) => c[0] === 'open_external');
      expect(open?.[1]).toEqual({ url: 'https://weread.qq.com/r/weread-skills' });
    });
  });

  it('空着提交会提示先贴上 Key，不以 wrk- 开头则拒绝', async () => {
    await renderEmpty();
    fireEvent.click(screen.getByRole('button', { name: '保存并同步' }));
    expect((await screen.findByRole('alert')).textContent).toContain('先贴上 Key。');
    expect(callMock.mock.calls.some((c) => c[0] === 'save_api_key')).toBe(false);

    fireEvent.change(screen.getByLabelText('微信读书 API Key'), { target: { value: 'sk-wrong' } });
    fireEvent.click(screen.getByRole('button', { name: '保存并同步' }));
    expect((await screen.findByRole('alert')).textContent).toContain('wrk-');
    expect(callMock.mock.calls.some((c) => c[0] === 'save_api_key')).toBe(false);
  });

  it('保存并同步把 Key 写下后立刻拉书，墙上出现自己的划线', async () => {
    await renderEmpty();
    fireEvent.change(screen.getByLabelText('微信读书 API Key'), { target: { value: 'wrk-test-key' } });
    fireEvent.click(screen.getByRole('button', { name: '保存并同步' }));

    expect(await screen.findByText('地方政府像一家公司。')).toBeTruthy();
    const save = callMock.mock.calls.find((c) => c[0] === 'save_api_key');
    expect(save?.[1]).toEqual({ key: 'wrk-test-key' });
    expect(callMock.mock.calls.some((c) => c[0] === 'sync_all')).toBe(true);
    expect(await screen.findByText(/接到墙上/)).toBeTruthy();
    expect(screen.getByRole('button', { name: /置身事内/ })).toBeTruthy();
  });

  it('只保存后停在「Key 已经存好」，等用户自己点同步', async () => {
    await renderEmpty();
    fireEvent.change(screen.getByLabelText('微信读书 API Key'), { target: { value: 'wrk-test-key' } });
    fireEvent.click(screen.getByRole('button', { name: '只保存' }));

    expect(await screen.findByText('Key 已经存好，墙上还是空的')).toBeTruthy();
    expect(screen.getByText(/第一次可能要一两分钟/)).toBeTruthy();
    expect(callMock.mock.calls.some((c) => c[0] === 'save_api_key')).toBe(true);
    expect(callMock.mock.calls.some((c) => c[0] === 'sync_all')).toBe(false);
    const wallSync = document.querySelector('.empty-setup button.primary');
    if (!wallSync) throw new Error('墙上同步钮未渲染');
    fireEvent.click(wallSync);
    expect(await screen.findByText('地方政府像一家公司。')).toBeTruthy();
  });

  it('保存失败时错误留在表单上，不离开创刊页', async () => {
    backend.saveError = '写入 Key 失败';
    await renderEmpty();
    fireEvent.change(screen.getByLabelText('微信读书 API Key'), { target: { value: 'wrk-test-key' } });
    fireEvent.click(screen.getByRole('button', { name: '保存并同步' }));
    expect((await screen.findByRole('alert')).textContent).toContain('写入 Key 失败');
    expect(screen.getByText('把微信读书的划线接到这面墙上')).toBeTruthy();
    expect(callMock.mock.calls.some((c) => c[0] === 'sync_all')).toBe(false);
  });
});

describe('其他空态', () => {
  it('检索无结果时说明原因，并可清空', async () => {
    backend.hasKey = true;
    backend.hasBooks = true;
    backend.books = [book];
    backend.cards = [card];
    render(<App />);
    expect(await screen.findByText('地方政府像一家公司。')).toBeTruthy();

    callMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'search_cards') return [];
      if (cmd === 'get_setup_status') return { hasKey: true, hasBooks: true };
      if (cmd === 'list_books') return [book];
      if (cmd === 'list_tags') return [];
      if (cmd === 'get_due_count') return 0;
      if (cmd === 'query_cards') return [card];
      if (cmd === 'count_cards') return 1;
      throw new Error(`测试未处理的命令: ${cmd}`);
    });
    fireEvent.change(screen.getByLabelText(/检索/), { target: { value: '不存在的句子' } });

    expect(await screen.findByText('没有找到「不存在的句子」')).toBeTruthy();
    expect(screen.getByText(/换个词再搜一次/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '清空检索' }));
    expect(await screen.findByText('地方政府像一家公司。')).toBeTruthy();
  });

  it('星标筛选为空时给清除动作', async () => {
    backend.hasKey = true;
    backend.hasBooks = true;
    backend.books = [book];
    backend.cards = [card];
    render(<App />);
    expect(await screen.findByText('地方政府像一家公司。')).toBeTruthy();

    callMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'query_cards') {
        const starred = (args?.filter as { starredOnly?: boolean } | undefined)?.starredOnly;
        return starred ? [] : [card];
      }
      if (cmd === 'count_cards') {
        const starred = (args?.filter as { starredOnly?: boolean } | undefined)?.starredOnly;
        return starred ? 0 : 1;
      }
      if (cmd === 'get_setup_status') return { hasKey: true, hasBooks: true };
      if (cmd === 'list_books') return [book];
      if (cmd === 'list_tags') return [];
      if (cmd === 'get_due_count') return 0;
      throw new Error(`测试未处理的命令: ${cmd}`);
    });

    fireEvent.click(screen.getByRole('button', { name: /星标项目/ }));
    expect(await screen.findByText('没有符合当前筛选的卡片')).toBeTruthy();
    expect(screen.getByText(/星标下暂时没有卡片/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '清除筛选' }));
    expect(await screen.findByText('地方政府像一家公司。')).toBeTruthy();
  });
});
