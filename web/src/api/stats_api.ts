import { withEnvelope } from './client';
import { getStats } from './mock_store';
import type { ApiEnvelope, StatsSnapshot } from '../types/api';

export async function getStatsRequest(): Promise<ApiEnvelope<StatsSnapshot>> {
  return withEnvelope(() => getStats(), { delay: 240 });
}
