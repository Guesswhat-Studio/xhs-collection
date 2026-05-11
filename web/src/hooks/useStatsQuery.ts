import { useQuery } from '@tanstack/react-query';
import { getStatsRequest } from '../api/stats_api';

export function useStatsQuery() {
  return useQuery({
    queryKey: ['stats'],
    queryFn: async () => {
      const response = await getStatsRequest();
      return response.data;
    },
  });
}
