// 设置页「四、语言模型」：默认关闭；选供应商后才露出地址/模型/Key；保存把草稿交给后端。
import { describe, expect, it, vi, beforeEach, afterEach, type Mock } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { SettingsView } from './App';
import { call, type LlmSettings } from './types';

afterEach(() => cleanup());

vi.mock('./types', async () => {
  const actual = await vi.importActual<typeof import('./types')>('./types');
  return { ...actual, call: vi.fn() };
});

const callMock = call as unknown as Mock;

const off: LlmSettings = { provider: 'off', baseUrl: '', model: '', hasKey: false };

function mockBackend(llm: LlmSettings = off) {
  let stored = llm;
  callMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case 'get_settings':
        return { lastFullSync: null, dataDir: '/tmp/mudflat' };
      case 'get_review_settings':
        return { batchSize: 20 };
      case 'get_llm_settings':
        return stored;
      case 'save_llm_settings': {
        const draft = args?.draft as { provider: LlmSettings['provider']; baseUrl: string; model: string; key: string };
        stored = {
          provider: draft.provider,
          baseUrl: draft.baseUrl || (draft.provider === 'openai' ? 'https://api.openai.com/v1' : draft.baseUrl),
          model: draft.model,
          hasKey: draft.provider !== 'off' && (!!draft.key || stored.hasKey),
        };
        return stored;
      }
      case 'clear_llm_settings':
        stored = off;
        return undefined;
      case 'test_llm_connection':
        return '连接成功：已找到模型 gpt-4o-mini';
      default:
        throw new Error(`测试未处理的命令: ${cmd}`);
    }
  });
}

describe('SettingsView 语言模型', () => {
  beforeEach(() => {
    callMock.mockReset();
    mockBackend();
  });

  it('默认关闭时不展示接口地址和 Key', async () => {
    render(<SettingsView onToast={() => {}} hasKey={false} onKeyChange={() => {}} />);
    await waitFor(() => expect(screen.getByText('当前未启用。')).toBeTruthy());
    expect(screen.queryByLabelText('语言模型接口地址')).toBeNull();
    expect(screen.queryByLabelText('语言模型 API Key')).toBeNull();
  });

  it('选 OpenAI 后填入默认地址，保存把草稿交给后端', async () => {
    const toasts: string[] = [];
    render(<SettingsView onToast={(m) => toasts.push(m)} hasKey={false} onKeyChange={() => {}} />);
    await waitFor(() => expect(screen.getByRole('button', { name: 'OpenAI' })).toBeTruthy());

    fireEvent.click(screen.getByRole('button', { name: 'OpenAI' }));
    const url = await screen.findByLabelText('语言模型接口地址');
    expect((url as HTMLInputElement).value).toBe('https://api.openai.com/v1');
    expect((screen.getByLabelText('语言模型名') as HTMLInputElement).value).toBe('gpt-4o-mini');

    fireEvent.change(screen.getByLabelText('语言模型 API Key'), { target: { value: 'sk-test' } });
    fireEvent.click(screen.getAllByRole('button', { name: '保存到本机' })[1]);

    await waitFor(() => {
      const save = callMock.mock.calls.find((c) => c[0] === 'save_llm_settings');
      expect(save).toBeTruthy();
      expect(save?.[1]).toEqual({
        draft: {
          provider: 'openai',
          baseUrl: 'https://api.openai.com/v1',
          model: 'gpt-4o-mini',
          key: 'sk-test',
        },
      });
    });
    expect(toasts.some((t) => t.includes('语言模型已保存'))).toBe(true);
  });

  it('从 OpenAI 切到 Ollama 时换成回环地址', async () => {
    render(<SettingsView onToast={() => {}} hasKey={false} onKeyChange={() => {}} />);
    await waitFor(() => expect(screen.getByRole('button', { name: 'OpenAI' })).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: 'OpenAI' }));
    await screen.findByLabelText('语言模型接口地址');
    fireEvent.click(screen.getByRole('button', { name: 'Ollama' }));
    expect((screen.getByLabelText('语言模型接口地址') as HTMLInputElement).value)
      .toBe('http://127.0.0.1:11434/v1');
  });
});
