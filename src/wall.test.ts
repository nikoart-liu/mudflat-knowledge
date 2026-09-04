// 卡片墙分组测试：总索引按书分组；书内按章节分组——用户进了一本书，
// 关注的只是这一本，心智结构是书的章节而非日历月份；章节缺失归入「未分章」。
import { describe, expect, it } from 'vitest';
import { buildChapterToc, buildWallGroups } from './App';
import type { CardRow } from './types';

let nextId = 1;
function card(p: Partial<CardRow> & { text: string }): CardRow {
  return {
    id: nextId++,
    kind: 'highlight',
    bookId: 1,
    remoteId: null,
    chapterUid: null,
    chapterTitle: null,
    abstractText: null,
    rangeStr: null,
    note: '',
    starred: false,
    excludedFromReview: false,
    createdAt: 1_700_000_000,
    updatedAt: 1_700_000_000,
    deleted: false,
    bookTitle: '置身事内',
    tags: [],
    ...p,
  };
}

describe('buildWallGroups', () => {
  it('总索引按书分组，空书名归入自建卡', () => {
    const groups = buildWallGroups(
      [
        card({ text: 'a', bookTitle: '置身事内' }),
        card({ text: 'b', bookTitle: '' }),
        card({ text: 'c', bookTitle: '置身事内' }),
        card({ text: 'd', bookTitle: '世间的回答' }),
      ],
      'book',
    );
    expect(groups.map((g) => [g.label, g.cards.length])).toEqual([
      ['置身事内', 2],
      ['自建卡', 1],
      ['世间的回答', 1],
    ]);
    expect(groups.every((g) => !g.mono)).toBe(true);
  });

  it('书内按章节分组，保持输入顺序（最近划过的章在前）', () => {
    const groups = buildWallGroups(
      [
        card({ text: 'a', chapterUid: 3, chapterTitle: '第三章 地方政府的公司化' }),
        card({ text: 'b', chapterUid: 1, chapterTitle: '第一章' }),
        card({ text: 'c', chapterUid: 3, chapterTitle: '第三章 地方政府的公司化' }),
      ],
      'chapter',
    );
    expect(groups.map((g) => [g.key, g.label, g.cards.length])).toEqual([
      ['c-3', '第三章 地方政府的公司化', 2],
      ['c-1', '第一章', 1],
    ]);
  });

  it('章节缺失或空标题归入「未分章」；章名两端空白先裁切', () => {
    const groups = buildWallGroups(
      [
        card({ text: 'a' }),
        card({ text: 'b', chapterUid: 5, chapterTitle: '  ' }),
        card({ text: 'c', chapterUid: 6, chapterTitle: ' 结语 ' }),
      ],
      'chapter',
    );
    expect(groups.map((g) => [g.key, g.label, g.cards.length])).toEqual([
      ['no-chapter', '未分章', 2],
      ['c-6', '结语', 1],
    ]);
  });

  it('空输入返回空数组', () => {
    expect(buildWallGroups([], 'book')).toEqual([]);
    expect(buildWallGroups([], 'chapter')).toEqual([]);
  });
});

describe('buildChapterToc', () => {
  it('按 chapterUid 升序排目次，未分章垫底；key 与墙分组对齐', () => {
    const cards = [
      card({ text: 'a', chapterUid: 3, chapterTitle: '第三章 地方政府的公司化' }),
      card({ text: 'b' }),
      card({ text: 'c', chapterUid: 1, chapterTitle: '第一章' }),
      card({ text: 'd', chapterUid: 3, chapterTitle: '第三章 地方政府的公司化' }),
      card({ text: 'e', chapterUid: 5, chapterTitle: '  ' }),
    ];
    expect(buildChapterToc(cards)).toEqual([
      { key: 'c-1', label: '第一章', count: 1 },
      { key: 'c-3', label: '第三章 地方政府的公司化', count: 2 },
      { key: 'no-chapter', label: '未分章', count: 2 },
    ]);
  });

  it('墙仍按最近划过的章在前，目次不跟墙走', () => {
    const cards = [
      card({ text: 'a', chapterUid: 3, chapterTitle: '第三章' }),
      card({ text: 'b', chapterUid: 1, chapterTitle: '第一章' }),
    ];
    expect(buildWallGroups(cards, 'chapter').map((g) => g.key)).toEqual(['c-3', 'c-1']);
    expect(buildChapterToc(cards).map((e) => e.key)).toEqual(['c-1', 'c-3']);
  });

  it('空输入返回空数组', () => {
    expect(buildChapterToc([])).toEqual([]);
  });
});
