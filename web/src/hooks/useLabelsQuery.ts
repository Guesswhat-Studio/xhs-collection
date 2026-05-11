import { useQuery } from '@tanstack/react-query';
import { listLabelsRequest } from '../api/labels_api';

export function useLabelsQuery() {
  return useQuery({
    queryKey: ['labels'],
    queryFn: async () => {
      const response = await listLabelsRequest();
      return response.data;
    },
  });
}
