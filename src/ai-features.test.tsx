// AI 问题面 / 混合检索 / 回忆支架 前端行为。
import { describe, expect, it, vi, beforeEach, afterEach, type Mock } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { EditModal, ReviewView, groupSearchHits } from './App';
import { call, type CardRow, type QuestionFace, type RelatedCard } from './types';

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
    bookTitle: '原子习惯',
    tags: [],
  };
}

const proposed: QuestionFace = {
  artifactId: 9,
  cardId: 1,
  status: 'proposed',
  content: {
    unsuitable: false,
    reason: null,
    candidates: [
      { kind: 'concept', question: '这段话如何解释身份与重复行为的关系？' },
      { kind: 'why', question: '为什么说人是被习惯塑造的？' },
    ],
    acceptedQuestion: null,
  },
  userEdited: false,
  stale: false,
  provider: 'openai',
  model: 'gpt-4o-mini',
  promptVersion: 'question-face-v1',
};

describe('groupSearchHits', () => {
  it('把 both 留在原词命中，semantic 单独成组', () => {
    const a = card(1, '沉没成本');
    const b = card(2, '继续投入');
    const c = card(3, '工作记忆');
    const kinds = new Map([
      [1, 'both' as const],
      [2, 'semantic' as const],
      [3, 'lexical' as const],
    ]);
    const groups = groupSearchHits([a, b, c], kinds);
    expect(groups.map((g) => g.label)).toEqual(['原词命中', '意思相关']);
    expect(groups[0].cards.map((x) => x.id)).toEqual([1, 3]);
    expect(groups[1].cards.map((x) => x.id)).toEqual([2]);
  });
});

describe('EditModal 问题面', () => {
  beforeEach(() => {
    callMock.mockReset();
    callMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'get_llm_settings':
          return { provider: 'openai', baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o-mini', embeddingModel: 'text-embedding-3-small', hasKey: true };
        case 'get_question_face':
          return null;
        case 'propose_question_face':
          return proposed;
        case 'accept_question_face':
          return {
            ...proposed,
            status: 'accepted',
            content: { ...proposed.content, acceptedQuestion: proposed.content.candidates[0].question },
          };
        case 'update_card':
          return undefined;
        default:
          throw new Error(`未处理: ${cmd}`);
      }
    });
  });

  it('生成候选后采用，不改原文', async () => {
    const toasts: string[] = [];
    render(
      <EditModal
        card={card(1, '人不是拥有习惯，而是由习惯塑造。')}
        onClose={() => {}}
        onSaved={() => {}}
        onToast={(m) => toasts.push(m)}
      />,
    );
    fireEvent.click(await screen.findByRole('button', { name: '生成问题面' }));
    expect(await screen.findByRole('button', { name: '采用' })).toBeTruthy();
    expect(screen.getByLabelText('问题面')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '采用' }));
    await waitFor(() => expect(callMock.mock.calls.some((c) => c[0] === 'accept_question_face')).toBe(true));
    expect(toasts.some((t) => t.includes('已采用'))).toBe(true);
    expect(screen.getByText(/人不是拥有习惯/)).toBeTruthy();
  });
});

describe('ReviewView 问题面与支架', () => {
  beforeEach(() => {
    callMock.mockReset();
  });

  it('有已采用问题面时，翻牌背面显示问题而不是编目号', async () => {
    callMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'get_due_count': return 1;
        case 'get_review_settings': return { batchSize: 20 };
        case 'get_due_cards': return [card(1, '人不是拥有习惯，而是由习惯塑造。')];
        case 'list_accepted_questions':
          return [{
            ...proposed,
            status: 'accepted',
            content: { ...proposed.content, acceptedQuestion: '这段话如何解释身份与重复行为的关系？' },
          }];
        case 'get_llm_settings':
          return { provider: 'off', baseUrl: '', model: '', embeddingModel: '', hasKey: false };
        default:
          throw new Error(`未处理: ${cmd}`);
      }
    });
    render(<ReviewView onToast={vi.fn()} onExit={vi.fn()} />);
    fireEvent.click(await screen.findByRole('button', { name: /开始翻牌/ }));
    expect(await screen.findByText('建议问题')).toBeTruthy();
    expect(screen.getByText('这段话如何解释身份与重复行为的关系？')).toBeTruthy();
    expect(screen.queryByText('01')).toBeNull();
  });

  it('评「忘了」后若有支架则暂停，下一张才前进', async () => {
    const graded: string[] = [];
    callMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      switch (cmd) {
        case 'get_due_count': return 2;
        case 'get_review_settings': return { batchSize: 20 };
        case 'get_due_cards': return [card(1, '第一条'), card(2, '第二条')];
        case 'list_accepted_questions': return [];
        case 'get_llm_settings':
          return { provider: 'off', baseUrl: '', model: '', embeddingModel: '', hasKey: false };
        case 'grade_review':
          graded.push(String(args?.rating));
          return {
            prev: { due_at: 0, interval_days: 0, ease: 2.5, reps: 0, lapses: 0 },
            next: { due_at: NOW + 600, interval_days: 0, ease: 2.3, reps: 0, lapses: 1 },
          };
        case 'get_review_scaffold':
          return {
            paraphrase: '身份是重复出来的。',
            example: null,
            neighbors: [card(9, '每做一个 1% 的改进。')],
            sourceCardIds: [1, 9],
            fromAi: false,
          };
        default:
          throw new Error(`未处理: ${cmd}`);
      }
    });
    const { container } = render(<ReviewView onToast={vi.fn()} onExit={vi.fn()} />);
    fireEvent.click(await screen.findByRole('button', { name: /开始翻牌/ }));
    await waitFor(() => expect(container.querySelector('.deck-back-num')?.textContent).toMatch(/01/));
    fireEvent.keyDown(window, { key: ' ' });
    await screen.findByRole('group', { name: '记忆评分' });
    fireEvent.keyDown(window, { key: '1' });
    expect(await screen.findByText('换个角度')).toBeTruthy();
    expect(screen.getByText('身份是重复出来的。')).toBeTruthy();
    expect(graded).toEqual(['again']);
    expect(container.querySelector('.deck-back-num')?.textContent).toMatch(/01/);
    fireEvent.click(screen.getByRole('button', { name: /下一张/ }));
    await waitFor(() => expect(container.querySelector('.deck-back-num')?.textContent).toMatch(/02/), { timeout: 2000 });
  });
});

describe('EditModal 相似卡', () => {
  it('找相似卡列出相关线索', async () => {
    const rel: RelatedCard[] = [{
      card: card(2, '已经投入的成本会让人加码。'),
      score: 0.7,
      reason: 'similar',
    }];
    callMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'get_llm_settings':
          return { provider: 'off', baseUrl: '', model: '', embeddingModel: '', hasKey: false };
        case 'get_question_face':
          return null;
        case 'get_related_cards':
          return rel;
        default:
          throw new Error(`未处理: ${cmd}`);
      }
    });
    render(
      <EditModal
        card={card(1, '沉没成本让人继续投入。')}
        onClose={() => {}}
        onSaved={() => {}}
        onToast={() => {}}
      />,
    );
    fireEvent.click(await screen.findByRole('button', { name: '找相似卡' }));
    expect(await screen.findByText('已经投入的成本会让人加码。')).toBeTruthy();
    expect(screen.queryByRole('button', { name: '生成问题面' })).toBeNull();
  });
});
