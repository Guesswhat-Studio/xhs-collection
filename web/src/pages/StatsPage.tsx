import { StatsCharts } from '../components/stats/StatsCharts';
import { StatsSummary } from '../components/stats/StatsSummary';
import { useStatsQuery } from '../hooks/useStatsQuery';

export function StatsPage() {
  const statsQuery = useStatsQuery();

  if (statsQuery.isLoading || !statsQuery.data) {
    return (
      <section className="stack-page">
        <div className="notes-skeleton" aria-hidden="true">
          {Array.from({ length: 4 }).map((_, index) => (
            <div key={index} className="notes-skeleton__card" />
          ))}
        </div>
      </section>
    );
  }

  return (
    <section className="stack-page">
      <div className="page-intro panel-surface">
        <p className="panel-header__eyebrow">Light review</p>
        <h3>用几个有判断价值的概览，帮自己知道接下来先整理什么</h3>
        <p>最后一次同步：{statsQuery.data.lastSyncedAt}。这里不追求炫技图表，只保留对回顾真正有帮助的分布。</p>
      </div>

      <StatsSummary stats={statsQuery.data} />
      <StatsCharts stats={statsQuery.data} />
    </section>
  );
}
