import type { ApiEnvelope } from '../types/api';

const wait = (duration: number) => new Promise((resolve) => window.setTimeout(resolve, duration));

export async function withEnvelope<T>(
  producer: () => Promise<T> | T,
  options?: { delay?: number; meta?: Record<string, unknown> },
): Promise<ApiEnvelope<T>> {
  if (options?.delay) {
    await wait(options.delay);
  }

  return {
    data: await producer(),
    error: null,
    meta: options?.meta,
  };
}
