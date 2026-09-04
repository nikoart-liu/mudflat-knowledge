// 侧栏书籍可见性：本期要目 + 置顶区 + 分类树（一级大类 → 书）+ 「新」点 + 置顶钮。
// 层级规则（用户定稿 2026-09-04）：分类只到大类一级，子类不再出层（数据键
// 「大类-子类」展示层取首段）；无分类归「未分类」挂尾；置顶书在树外独立成区、
// 计入大类合计；目录号是终身编目号（入馆序分配，置顶/折叠不改号）。
// 后端行为（pinned 排序/防覆盖、recent_card_at）在 src-tauri 的 db 测试覆盖。
import { describe, expect, it, vi, beforeEach, afterEach, beforeAll, type Mock } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import App from './App';
import { call, emptyLlmSettings, type BookRow, type CardRow } from './types';

afterEach(() => cleanup());

vi.mock('./types', async () => {
  const actual = await vi.importActual<typeof import('./types')>('./types');
  return { ...actual, call: vi.fn() };
});

const callMock = call as unknown as Mock;
const NOW = 1_700_000_000;
const HOUR_AGO = Math.floor(Date.now() / 1000) - 3600;

const base = { author: '', cover: '', readingProgress: 0, syncReviews: true, syncedAt: null };
const books: BookRow[] = [
  { id: 1, wereadBookId: 'w1', title: '置身事内', noteCount: 2, reviewCount: 0,
    ...base, pinned: true, category: '经济理财-理财', recentCardAt: HOUR_AGO },
  { id: 2, wereadBookId: 'w2', title: '纳瓦尔宝典', noteCount: 121, reviewCount: 0,
    ...base, pinned: false, category: '经济理财-商业', recentCardAt: null },
  { id: 3, wereadBookId: 'w3', title: '专业投机原理', noteCount: 101, reviewCount: 0,
    ...base, pinned: false, category: '经济理财-商业', recentCardAt: null },
  { id: 4, wereadBookId: 'w4', title: '夜晚的潜水艇', noteCount: 2, reviewCount: 0,
    ...base, pinned: false, category: '文学-散文杂著', recentCardAt: null },
  { id: 5, wereadBookId: 'w5', title: '自建笔记', noteCount: 0, reviewCount: 0,
    ...base, pinned: false, category: '', recentCardAt: null },
];

function card(id: number, text: string): CardRow {
  return {
    id, kind: 'highlight', bookId: 1, remoteId: `r-${id}`, chapterUid: 1,
    chapterTitle: '第一章', text, abstractText: null, rangeStr: null,
    note: '', starred: false, excludedFromReview: false, createdAt: NOW, updatedAt: NOW,
    deleted: false, bookTitle: '置身事内', tags: [],
  };
}

beforeAll(() => {
  window.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

beforeEach(() => {
  callMock.mockReset();
  localStorage.clear(); // 折叠记忆等本地状态不跨用例泄漏
  // set_book_pinned 真正改状态，list_books 按状态返回——模拟后端语义
  const pinned = new Map<number, boolean>();
  callMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case 'get_setup_status':
        return { hasKey: true, hasBooks: true };
      case 'list_books':
        return books.map((b) => (pinned.has(b.id) ? { ...b, pinned: pinned.get(b.id)! } : b));
      case 'set_book_pinned':
        pinned.set(args?.bookId as number, args?.pinned as boolean);
        return null;
      case 'list_tags':
        return [];
      case 'get_due_count':
        return 0;
      case 'query_cards':
        return [card(1, '工作记忆容量有限。')];
      case 'count_cards':
        return 1;
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

// 稳定锚点：等末位书行出现 = list_books 已渲染（书名在文件内唯一）
const lastBookRow = () => screen.findByRole('button', { name: /自建笔记/ });

describe('侧栏分类层级树', () => {
  it('一级大类归类：无分类归「未分类」挂尾，按体量降序，二级分类不再出现', async () => {
    const { container } = render(<App />);
    await lastBookRow();

    // 大类眉标按体量降序：经济理财(224，含置顶书) > 文学(2) > 未分类(0)
    const groupNames = [...container.querySelectorAll('.cat-eyebrow .cat-name')].map((el) => el.textContent);
    expect(groupNames).toEqual(['经济理财', '文学', '未分类']);

    // 二级分类已移除：数据键里的子类（理财/商业/散文杂著）不出层
    expect(container.querySelectorAll('.cat-sub')).toHaveLength(0);

    // 右缘计数：合计计全量（置顶书计入——「合计」名实相符）
    const counts = [...container.querySelectorAll('.cat-eyebrow .cat-count')].map((el) => el.textContent);
    expect(counts).toEqual(['224', '2', '0']);

    // 要目组在置顶区之前；置顶眉标带计数
    expect([...container.querySelectorAll('.sub-eyebrow')].map((el) => el.textContent)).toEqual(['本期要目', '置顶 · 1']);
  });

  it('编目号按入馆序全书分配：同一本书在要目/置顶区同号', async () => {
    const { container } = render(<App />);
    await lastBookRow();

    // 置身事内（入馆最早）在要目与置顶区各出现一次，编目号同为 01
    const nos = [...container.querySelectorAll('.book-no')].map((el) => el.textContent);
    expect(nos).toEqual(['01', '01', '02', '03', '04', '05']);

    const titles = [...container.querySelectorAll('.book-title')].map((el) => el.textContent);
    expect(titles).toEqual(['置身事内', '置身事内', '纳瓦尔宝典', '专业投机原理', '夜晚的潜水艇', '自建笔记']);
  });

  it('「新」点只标最近有新划线的书；分类小字只在置顶行出现（大类名）', async () => {
    const { container } = render(<App />);
    await lastBookRow();

    expect(container.querySelectorAll('.book-new')).toHaveLength(1);

    // 置顶行脱离分组，小字补回大类名；树内书行紧贴眉标，不重复
    const cats = [...container.querySelectorAll('.book-cat')].map((el) => el.textContent);
    expect(cats).toEqual(['经济理财']);
  });

  it('置顶钮把书移入树外置顶区；置顶行小字补大类名', async () => {
    const { container } = render(<App />);
    await lastBookRow();

    fireEvent.click(screen.getByRole('button', { name: /纳瓦尔宝典/ }));
    const pinBtn = await screen.findByRole('button', { name: '置顶' });
    fireEvent.click(pinBtn);

    expect(callMock).toHaveBeenCalledWith('set_book_pinned', { bookId: 2, pinned: true });
    await screen.findByRole('button', { name: '已置顶' });

    // 两本置顶行都补回大类名（数据键「经济理财-理财/商业」取首段）
    const cats = [...container.querySelectorAll('.book-cat')].map((el) => el.textContent);
    expect(cats).toEqual(['经济理财', '经济理财']);

    const titles = [...container.querySelectorAll('.book-title')].map((el) => el.textContent);
    expect(titles).toEqual(['置身事内', '置身事内', '纳瓦尔宝典', '专业投机原理', '夜晚的潜水艇', '自建笔记']);
  });

  it('大类可折叠：折叠隐藏组内书行，编目号不重排，状态写入本地记忆', async () => {
    const { container } = render(<App />);
    await lastBookRow();

    // 折叠大类「经济理财」
    fireEvent.click(screen.getByRole('button', { name: /^经济理财/ }));
    expect(screen.getByRole('button', { name: /^经济理财/ }).getAttribute('aria-expanded')).toBe('false');

    // 状态写入本地记忆（跨会话恢复），键为大类名
    const stored = JSON.parse(localStorage.getItem('mudflat.collapsed-cats') ?? '[]') as string[];
    expect(stored).toContain('经济理财');

    // 组内两本被隐藏，要目/置顶与其它组不受影响
    const titles = [...container.querySelectorAll('.book-title')].map((el) => el.textContent);
    expect(titles).toEqual(['置身事内', '置身事内', '夜晚的潜水艇', '自建笔记']);

    // 编目号是入馆序，折叠不重排
    const nos = [...container.querySelectorAll('.book-no')].map((el) => el.textContent);
    expect(nos).toEqual(['01', '01', '04', '05']);

    // 再点展开，五本齐（要目 + 置顶各一次置身事内 = 六行）；展开后记忆清空该键
    fireEvent.click(screen.getByRole('button', { name: /^经济理财/ }));
    const expanded = [...container.querySelectorAll('.book-title')].map((el) => el.textContent);
    expect(expanded).toHaveLength(6);
    expect(JSON.parse(localStorage.getItem('mudflat.collapsed-cats') ?? '[]')).not.toContain('经济理财');
  });

  it('编目号是终身号：把最后一本置顶，任何人的号都不动', async () => {
    const { container } = render(<App />);
    await lastBookRow();

    fireEvent.click(screen.getByRole('button', { name: /自建笔记/ }));
    const pinBtn = await screen.findByRole('button', { name: '置顶' });
    fireEvent.click(pinBtn);
    await screen.findByRole('button', { name: '已置顶' });

    // 版面位置变了（自建笔记升到置顶区），入馆号原地不动：01 01 05 02 03 04
    const nos = [...container.querySelectorAll('.book-no')].map((el) => el.textContent);
    expect(nos).toEqual(['01', '01', '05', '02', '03', '04']);
  });
});

describe('侧栏检索态与可达性', () => {
  it('范围受限检索：书架顶出现范围眉标；星标行 active 与 aria-pressed 同步；搜全部后退场', async () => {
    const { container } = render(<App />);
    await lastBookRow();

    fireEvent.click(screen.getByRole('button', { name: /星标项目/ }));
    const input = screen.getByRole('searchbox');
    fireEvent.change(input, { target: { value: '记忆' } });
    await screen.findByText(/检索范围/);

    // 检索中位置信号不熄灯：active 类与 aria-pressed 说同一句话
    const star = screen.getByRole('button', { name: /星标项目/ });
    expect(star.className).toContain('active');
    expect(star.getAttribute('aria-pressed')).toBe('true');
    expect(container.querySelector('.scope-eyebrow')?.textContent).toContain('星标');

    // 切「搜索全部卡片」：范围眉标退场（全库检索无需声明范围）
    fireEvent.click(screen.getByRole('button', { name: '搜索全部卡片' }));
    await waitFor(() => expect(container.querySelector('.scope-eyebrow')).toBeNull());
  });

  it('金方点 role=img 带命名；分类小字不再 aria-hidden（置顶行可访问名含大类）', async () => {
    const { container } = render(<App />);
    await lastBookRow();

    const dot = container.querySelector('.book-new');
    expect(dot?.getAttribute('role')).toBe('img');
    expect(dot?.getAttribute('aria-label')).toBe('最近 7 天有新划线');
    expect(container.querySelector('.book-cat')?.hasAttribute('aria-hidden')).toBe(false);

    // 置顶书行的可访问名现在携带大类名（要目行与置顶行同书名，取含分类串的那行）
    const rows = screen.getAllByRole('button', { name: /置身事内/ });
    expect(rows.some((r) => (r.textContent ?? '').includes('经济理财'))).toBe(true);
  });

  it('键盘漫游：↑↓ 循环移动焦点、End 跳尾、← 收起分组、连打书名首字跳书', async () => {
    const { container } = render(<App />);
    await lastBookRow();

    const rows = () => [...container.querySelectorAll<HTMLElement>('.side-scroll button')];
    rows()[0].focus();
    expect(document.activeElement).toBe(rows()[0]);

    fireEvent.keyDown(rows()[0], { key: 'ArrowDown' });
    expect(document.activeElement).toBe(rows()[1]);

    fireEvent.keyDown(document.activeElement as HTMLElement, { key: 'End' });
    expect(document.activeElement).toBe(rows()[rows().length - 1]);

    // ← 在分组眉标上收起（方向即语义）
    const eyebrow = screen.getByRole('button', { name: /^经济理财/ });
    eyebrow.focus();
    fireEvent.keyDown(eyebrow, { key: 'ArrowLeft' });
    expect(eyebrow.getAttribute('aria-expanded')).toBe('false');

    // type-ahead：连打「夜」直接跳到《夜晚的潜水艇》
    fireEvent.keyDown(eyebrow, { key: '夜' });
    expect(document.activeElement).toBe(screen.getByRole('button', { name: /夜晚的潜水艇/ }));
  });
});
