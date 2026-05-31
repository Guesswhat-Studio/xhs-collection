import { useEffect, useState } from 'react';
import { Icon } from './icons';

type IdleWindow = Window & typeof globalThis & {
  requestIdleCallback?: (callback: () => void, options?: { timeout?: number }) => number;
  cancelIdleCallback?: (handle: number) => void;
};

function scheduleIdleRender(callback: () => void) {
  const idleWindow = window as IdleWindow;
  if (idleWindow.requestIdleCallback && idleWindow.cancelIdleCallback) {
    const handle = idleWindow.requestIdleCallback(callback, { timeout: 180 });
    return () => idleWindow.cancelIdleCallback?.(handle);
  }
  const handle = window.setTimeout(callback, 16);
  return () => window.clearTimeout(handle);
}

export function useProgressiveItems<T>(items: T[], initialCount: number, stepCount: number) {
  const safeInitial = Math.max(1, initialCount);
  const safeStep = Math.max(1, stepCount);
  const [state, setState] = useState(() => ({
    count: Math.min(items.length, safeInitial),
    initial: safeInitial,
    items,
  }));
  const visibleCount =
    state.items === items && state.initial === safeInitial
      ? Math.min(state.count, items.length)
      : Math.min(items.length, safeInitial);

  useEffect(() => {
    const firstCount = Math.min(items.length, safeInitial);
    setState({ count: firstCount, initial: safeInitial, items });
    if (items.length <= firstCount) return undefined;

    let disposed = false;
    let cancelScheduled: (() => void) | undefined;

    function revealMore() {
      if (disposed) return;
      setState((current) => {
        if (current.items !== items) return current;
        const nextCount = Math.min(items.length, current.count + safeStep);
        if (nextCount < items.length && !disposed) {
          cancelScheduled = scheduleIdleRender(revealMore);
        }
        return { ...current, count: nextCount };
      });
    }

    cancelScheduled = scheduleIdleRender(revealMore);
    return () => {
      disposed = true;
      cancelScheduled?.();
    };
  }, [items, safeInitial, safeStep]);

  return {
    hasMore: visibleCount < items.length,
    items: items.slice(0, visibleCount),
    visibleCount,
  };
}

export function ProgressiveListTail({ shown, total }: { shown: number; total: number }) {
  if (shown >= total) return null;
  return (
    <div className="progressive-tail" aria-live="polite">
      <Icon name="loader" size={14} className="spin" />
      正在载入 {shown} / {total}
    </div>
  );
}
