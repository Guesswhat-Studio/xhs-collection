import type { StatsSnapshot } from '../../types/api';

interface StatsChartsProps {
  stats: StatsSnapshot;
}

function BarGroup({ items }: { items: { label: string; count: number }[] }) {
  const max = Math.max(...items.map((item) => item.count), 1);

  return (
    <div className="bar-group">
      {items.map((item) => (
        <div key={item.label} className="bar-group__row">
          <span>{item.label}</span>
          <div className="bar-group__track">
            <div className="bar-group__fill" style={{ width: `${(item.count / max) * 100}%` }} />
          </div>
          <strong>{item.count}</strong>
        </div>
      ))}
    </div>
  );
}

export function StatsCharts({ stats }: StatsChartsProps) {
  return (
    <section className="stats-grid">
      <article className="panel-surface stats-panel">
        <div className="panel-header">
          <div>
            <p className="panel-header__eyebrow">Staleness overview</p>
            <h3>哪些内容该优先复核</h3>
          </div>
        </div>
        <BarGroup
          items={[
            { label: 'Fresh', count: stats.freshCount },
            { label: 'Aging', count: stats.agingCount },
            { label: 'Outdated', count: stats.outdatedCount },
          ]}
        />
      </article>

      <article className="panel-surface stats-panel">
        <div className="panel-header">
          <div>
            <p className="panel-header__eyebrow">Topic distribution</p>
            <h3>当前收藏主要集中在哪些主题</h3>
          </div>
        </div>
        <BarGroup items={stats.bySystemLabel} />
      </article>

      <article className="panel-surface stats-panel">
        <div className="panel-header">
          <div>
            <p className="panel-header__eyebrow">Account mix</p>
            <h3>多账号内容目前如何分布</h3>
          </div>
        </div>
        <BarGroup items={stats.byAccount} />
      </article>
    </section>
  );
}
