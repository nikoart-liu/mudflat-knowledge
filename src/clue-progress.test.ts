import { describe, expect, it } from 'vitest';
import { formatClueElapsed, formatClueProgress } from './clue-progress';
import type { MindmapEventPayload } from './types';

function ev(partial: Partial<MindmapEventPayload>): MindmapEventPayload {
  return { stage: '', current: 0, total: 0, title: '', message: '', ...partial };
}

describe('formatClueProgress', () => {
  it('start 用后台给的计划说明', () => {
    expect(formatClueProgress(ev({ stage: 'start', message: '186 张卡片，按 8 章归纳' })))
      .toBe('186 张卡片，按 8 章归纳…');
  });

  it('oneshot 标明正在请求模型', () => {
    expect(formatClueProgress(ev({ stage: 'oneshot', message: '64 张' })))
      .toBe('正在请求模型（64 张）…');
  });

  it('chapter 报正在归纳的章名', () => {
    expect(formatClueProgress(ev({ stage: 'chapter', current: 2, total: 8, title: '环境' })))
      .toBe('正在归纳「环境」…');
  });

  it('chapter_failed 带上失败原因', () => {
    expect(formatClueProgress(ev({
      stage: 'chapter_failed', title: '坏习惯', message: '连接被对端断开',
    }))).toBe('「坏习惯」失败：连接被对端断开');
  });

  it('retry 原样展示后台句子', () => {
    expect(formatClueProgress(ev({ stage: 'retry', message: '连接不稳，第 2 次尝试…' })))
      .toBe('连接不稳，第 2 次尝试…');
  });

  it('merge 与 sanitize 是固定短句', () => {
    expect(formatClueProgress(ev({ stage: 'merge' }))).toBe('合并各章概要…');
    expect(formatClueProgress(ev({ stage: 'sanitize' }))).toBe('清洗并保存…');
  });
});

describe('formatClueElapsed', () => {
  it('不到一秒不显示', () => {
    expect(formatClueElapsed(0)).toBe('');
  });

  it('秒与分用中文计量', () => {
    expect(formatClueElapsed(3)).toBe('已等待 3 秒');
    expect(formatClueElapsed(60)).toBe('已等待 1 分');
    expect(formatClueElapsed(75)).toBe('已等待 1 分 15 秒');
  });
});
