import { BarChart3, FolderKanban, NotebookTabs, Tags, Waypoints } from 'lucide-react';
import { NavLink } from 'react-router-dom';

const items = [
  { to: '/notes', label: 'Notes', description: '搜索、筛选、整理', icon: NotebookTabs },
  { to: '/albums', label: 'Albums', description: '把零散收藏编成主题', icon: FolderKanban },
  { to: '/labels', label: 'Labels', description: '把自己的标签系统养起来', icon: Tags },
  { to: '/stats', label: 'Statistics', description: '轻量回顾，不做厚重看板', icon: BarChart3 },
];

export function SidebarNav() {
  return (
    <aside className="sidebar-nav">
      <div className="sidebar-nav__brand">
        <div className="sidebar-nav__monogram">
          <Waypoints size={18} />
        </div>
        <div>
          <p className="sidebar-nav__eyebrow">local collection viewer</p>
          <h1>xhs-collection</h1>
        </div>
      </div>

      <div className="sidebar-nav__memory-card">
        <span>Collect</span>
        <span>Categorise</span>
        <span>Compilation</span>
      </div>

      <nav className="sidebar-nav__links" aria-label="Primary">
        {items.map(({ to, label, description, icon: Icon }) => (
          <NavLink
            key={to}
            to={to}
            className={({ isActive }) => `sidebar-nav__link${isActive ? ' is-active' : ''}`}
          >
            <div className="sidebar-nav__icon-wrap">
              <Icon size={18} />
            </div>
            <div>
              <strong>{label}</strong>
              <span>{description}</span>
            </div>
          </NavLink>
        ))}
      </nav>

      <div className="sidebar-nav__footer">
        <p>桌面优先 · 本地优先</p>
        <small>让收藏不靠记忆，也不需要一直往下翻。</small>
      </div>
    </aside>
  );
}
