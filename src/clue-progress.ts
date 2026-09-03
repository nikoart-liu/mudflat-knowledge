import type { MindmapEventPayload } from './types';

export function formatClueProgress(ev: MindmapEventPayload): string {
  switch (ev.stage) {
    case 'start':
      return ev.message ? `${ev.message}…` : '整理卡片…';
    case 'oneshot':
      return ev.message ? `正在请求模型（${ev.message}）…` : '正在请求模型…';
    case 'chapter':
      return `正在归纳「${ev.title}」…`;
    case 'chapter_ok':
      return `「${ev.title}」已归纳`;
    case 'chapter_failed':
      return ev.message
        ? `「${ev.title}」失败：${ev.message}`
        : `「${ev.title}」归纳失败，继续下一章`;
    case 'chapter_skip':
      return `「${ev.title}」卡片不足，跳过`;
    case 'merge':
      return '合并各章概要…';
    case 'sanitize':
      return '清洗并保存…';
    case 'retry':
      return ev.message || `连接不稳，正在重试（${ev.current}）…`;
    case 'done':
      return '';
    default:
      return ev.message || '归纳中…';
  }
}

export function formatClueElapsed(sec: number): string {
  if (sec < 1) return '';
  if (sec < 60) return `已等待 ${sec} 秒`;
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  if (s === 0) return `已等待 ${m} 分`;
  return `已等待 ${m} 分 ${s} 秒`;
}
