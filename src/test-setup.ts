// vitest 环境装配：Node 26 的实验性 localStorage 未启用时（无 --localstorage-file），
// jsdom 窗口上也拿不到 localStorage。这里补一个内存版，保证 ReviewView 的
// 「首次移出回顾说明」逻辑可测、不依赖真实存储。
if (typeof globalThis.localStorage === 'undefined') {
  const store = new Map<string, string>();
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: {
      getItem: (k: string) => (store.has(k) ? (store.get(k) as string) : null),
      setItem: (k: string, v: string) => { store.set(k, String(v)); },
      removeItem: (k: string) => { store.delete(k); },
      clear: () => { store.clear(); },
      key: (i: number) => Array.from(store.keys())[i] ?? null,
      get length() { return store.size; },
    },
  });
}

export {};
