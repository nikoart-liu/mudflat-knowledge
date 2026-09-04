// 设置页「四、语言模型」：默认关闭；选供应商后才露出地址/模型/Key；保存把草稿交给后端。
// 「六、关于」：对照 GitHub Release 检查更新。
import { describe, expect, it, vi, beforeEach, afterEach, type Mock } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { SettingsView } from './App';
import { call, emptyLlmSettings, type LlmSettings, type UpdateInfo } from './types';

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

const off: LlmSettings = emptyLlmSettings();

function mockBackend(llm: LlmSettings = off) {
  let stored = llm;
  callMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case 'get_settings':
        return { lastFullSync: null, dataDir: '/tmp/mudflat', appVersion: '0.1.0' };
      case 'get_review_settings':
        return { batchSize: 20 };
      case 'get_llm_settings':
        return stored;
      case 'save_llm_settings': {
        const draft = args?.draft as { provider: LlmSettings['provider']; baseUrl: string; model: string; key: string };
        stored = {
          ...stored,
          provider: draft.provider,
          baseUrl: draft.baseUrl || (draft.provider === 'openai' ? 'https://api.openai.com/v1' : draft.baseUrl),
          model: draft.model,
          hasKey: draft.provider !== 'off' && (!!draft.key || stored.hasKey),
        };
        return stored;
      }
      case 'save_embedding_settings': {
        const draft = args?.draft as { provider: LlmSettings['provider']; baseUrl: string; model: string; key: string };
        stored = {
          ...stored,
          embeddingProvider: draft.provider,
          embeddingBaseUrl: draft.baseUrl || (draft.provider === 'openai' ? 'https://api.openai.com/v1' : draft.baseUrl),
          embeddingModel: draft.model
            || (draft.provider === 'openai' ? 'text-embedding-3-small' : draft.model),
          hasEmbeddingKey: draft.provider !== 'off' && (!!draft.key || stored.hasEmbeddingKey || stored.hasKey),
        };
        return stored;
      }
      case 'clear_llm_settings':
        stored = { ...stored, provider: 'off', baseUrl: '', model: '', hasKey: false };
        return stored;
      case 'clear_embedding_settings':
        stored = { ...stored, embeddingProvider: 'off', embeddingBaseUrl: '', embeddingModel: '', hasEmbeddingKey: false };
        return stored;
      case 'test_llm_connection':
        return '连接成功：已找到模型 gpt-4o-mini';
      case 'test_embedding_connection':
        return '连接成功：向量模型 text-embedding-3-small（1536 维）';
      case 'get_ai_index':
        return { embeddings: 0, artifacts: 0, providerOff: true, embeddingReady: false };
      case 'check_for_update':
        return { currentVersion: '0.1.0', latestVersion: '0.1.0', available: false, notes: '', htmlUrl: '', assetName: null, assetUrl: null };
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
    await waitFor(() => expect(screen.getByText(/默认关闭/)).toBeTruthy());
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

  it('选 xAI 后向量检索仍可单独配置 OpenAI', async () => {
    const toasts: string[] = [];
    render(<SettingsView onToast={(m) => toasts.push(m)} hasKey={false} onKeyChange={() => {}} />);
    fireEvent.click(await screen.findByRole('button', { name: 'xAI' }));
    expect(await screen.findByLabelText('语言模型接口地址')).toBeTruthy();
    expect(screen.queryByLabelText('向量模型名')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: '向量 · OpenAI' }));
    const embed = await screen.findByLabelText('向量模型名');
    expect((embed as HTMLInputElement).value).toBe('text-embedding-3-small');
    expect((screen.getByLabelText('向量模型接口地址') as HTMLInputElement).value)
      .toBe('https://api.openai.com/v1');

    fireEvent.change(screen.getByLabelText('向量模型 API Key'), { target: { value: 'sk-embed' } });
    fireEvent.click(screen.getByRole('button', { name: '保存向量配置' }));

    await waitFor(() => {
      const save = callMock.mock.calls.find((c) => c[0] === 'save_embedding_settings');
      expect(save?.[1]).toEqual({
        draft: {
          provider: 'openai',
          baseUrl: 'https://api.openai.com/v1',
          model: 'text-embedding-3-small',
          key: 'sk-embed',
        },
      });
    });
    expect(toasts.some((t) => t.includes('向量模型已保存'))).toBe(true);
  });

  it('语言模型关闭时仍能配置向量检索', async () => {
    render(<SettingsView onToast={() => {}} hasKey={false} onKeyChange={() => {}} />);
    await waitFor(() => expect(screen.getByText(/默认关闭/)).toBeTruthy());
    expect(screen.queryByLabelText('语言模型接口地址')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '向量 · Ollama' }));
    expect((screen.getByLabelText('向量模型接口地址') as HTMLInputElement).value)
      .toBe('http://127.0.0.1:11434/v1');
    expect((screen.getByLabelText('向量模型名') as HTMLInputElement).value).toBe('nomic-embed-text');
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

const newer: UpdateInfo = {
  currentVersion: '0.1.0',
  latestVersion: '0.2.0',
  available: true,
  notes: '修复同步。',
  htmlUrl: 'https://github.com/nikoart-liu/mudflat-knowledge/releases/tag/v0.2.0',
  assetName: 'mudflat-knowledge_0.2.0_darwin_aarch64.dmg',
  assetUrl: 'https://github.com/nikoart-liu/mudflat-knowledge/releases/download/v0.2.0/mudflat-knowledge_0.2.0_darwin_aarch64.dmg',
};

describe('SettingsView 关于与更新', () => {
  beforeEach(() => {
    callMock.mockReset();
    mockBackend();
  });

  it('关于里写出当前版本', async () => {
    render(<SettingsView onToast={() => {}} hasKey={false} onKeyChange={() => {}} />);
    expect(await screen.findByText(/当前版本 0\.1\.0/)).toBeTruthy();
    expect(screen.getByRole('button', { name: '检查更新' })).toBeTruthy();
  });

  it('GitHub 有新版本时露出下载并安装', async () => {
    const inner = callMock.getMockImplementation();
    callMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'check_for_update') return newer;
      return inner!(cmd, args);
    });

    render(<SettingsView onToast={() => {}} hasKey={false} onKeyChange={() => {}} />);
    fireEvent.click(await screen.findByRole('button', { name: '检查更新' }));
    expect(await screen.findByText('有新版本 0.2.0')).toBeTruthy();
    expect(screen.getByText('修复同步。')).toBeTruthy();
    expect(screen.getByRole('button', { name: '下载并安装' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '查看发布页' })).toBeTruthy();
  });

  it('已是最新时只提示，不出现安装钮', async () => {
    const inner = callMock.getMockImplementation();
    callMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'check_for_update') {
        return {
          ...newer,
          latestVersion: '0.1.0',
          available: false,
          assetName: null,
          assetUrl: null,
        };
      }
      return inner!(cmd, args);
    });

    render(<SettingsView onToast={() => {}} hasKey={false} onKeyChange={() => {}} />);
    fireEvent.click(await screen.findByRole('button', { name: '检查更新' }));
    expect(await screen.findByText('已是最新版本')).toBeTruthy();
    expect(screen.queryByRole('button', { name: '下载并安装' })).toBeNull();
  });

  it('下载并安装把进度通道交给后端', async () => {
    const inner = callMock.getMockImplementation();
    callMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'check_for_update') return newer;
      if (cmd === 'install_update') return '已打开安装包。装好后重新打开应用即可；本地卡片仍在本机，不会丢掉。';
      return inner!(cmd, args);
    });
    const toasts: string[] = [];
    render(<SettingsView onToast={(m) => toasts.push(m)} hasKey={false} onKeyChange={() => {}} />);
    fireEvent.click(await screen.findByRole('button', { name: '检查更新' }));
    fireEvent.click(await screen.findByRole('button', { name: '下载并安装' }));
    await waitFor(() => {
      const inst = callMock.mock.calls.find((c) => c[0] === 'install_update');
      expect(inst).toBeTruthy();
      expect(inst?.[1]).toEqual(expect.objectContaining({ onProgress: expect.anything() }));
    });
    expect(toasts.some((t) => t.includes('已打开安装包'))).toBe(true);
  });
});
