// Tauri invoke 重导出：后端错误本就是字符串，调用方直接 catch 展示
export { invoke as call } from '@tauri-apps/api/core';
// 后端命令的类型镜像。字段名与 Rust serde(rename_all=camelCase) 对应。

export interface BookRow {
  id: number;
  wereadBookId: string;
  title: string;
  author: string;
  cover: string;
  readingProgress: number;
  noteCount: number;
  reviewCount: number;
  syncReviews: boolean;
  syncedAt: number | null;
}

export interface CardRow {
  id: number;
  kind: 'highlight' | 'thought' | 'self';
  bookId: number | null;
  remoteId: string | null;
  chapterUid: number | null;
  chapterTitle: string | null;
  text: string;
  abstractText: string | null;
  rangeStr: string | null;
  note: string;
  starred: boolean;
  excludedFromReview: boolean;
  createdAt: number;
  updatedAt: number;
  deleted: boolean;
  bookTitle: string;
  tags: string[];
}

export interface CardFilter {
  bookId?: number | null;
  tagIds: number[];
  starredOnly: boolean;
  kinds: string[];
}

export function emptyFilter(): CardFilter {
  return { bookId: null, tagIds: [], starredOnly: false, kinds: [] };
}

export interface TagRow {
  id: number;
  name: string;
}

export interface SetupStatus {
  hasKey: boolean;
  hasBooks: boolean;
}

export interface SettingsInfo {
  lastFullSync: number | null;
  dataDir: string | null;
}

export interface SyncEventPayload {
  stage: string;
  current: number;
  total: number;
  bookTitle: string;
}

export interface FailedBook {
  bookId: string;
  title: string;
  error: string;
}

export interface SyncSummary {
  booksTotal: number;
  booksSynced: number;
  booksFailed: number;
  failures: FailedBook[];
  highlights: number;
  thoughts: number;
  added: number;
  removed: number;
}

export type ReviewRating = 'again' | 'hard' | 'good' | 'easy';

// 后端 srs::SrsState 未加 rename_all，字段保持 snake_case
export interface SrsState {
  due_at: number;
  interval_days: number;
  ease: number;
  reps: number;
  lapses: number;
}

export interface ReviewSettings {
  batchSize: number;
}

