import { withEnvelope } from './client';
import { listLabels } from './mock_store';
import type { ApiEnvelope } from '../types/api';
import type { LabelStat } from '../types/label';

export async function listLabelsRequest(): Promise<ApiEnvelope<LabelStat[]>> {
  return withEnvelope(() => listLabels(), { delay: 160 });
}
