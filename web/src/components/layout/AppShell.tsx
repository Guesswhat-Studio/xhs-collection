import { Sparkles } from 'lucide-react';
import { useLocation } from 'react-router-dom';
import type { PropsWithChildren } from 'react';
import { SidebarNav } from './SidebarNav';

const routeCopy: Record<string, { title: string; subtitle: string }> = {
  '/notes': {
    title: 'Notes workspace',
    subtitle: '先缩小范围，再开始整理。',
  },
  '/albums': {
    title: 'Albums studio',
    subtitle: '把一堆好内容，排成下次真的会回看的专题。',
  },
  '/labels': {
    title: 'Labels system',
    subtitle: '系统分类帮你打底，用户标签才真正贴近自己的回看习惯。',
  },
  '/stats': {
    title: 'Statistics review',
    subtitle: '只看有帮助的概览，不把整理空间做成企业 BI。',
  },
};

export function AppShell({ children }: PropsWithChildren) {
  const location = useLocation();
  const current = routeCopy[location.pathname] ?? routeCopy['/notes'];

  return (
    <div className="app-shell">
      <SidebarNav />
      <main className="app-shell__main">
        <header className="app-shell__header">
          <div>
            <p className="app-shell__eyebrow">warm minimal premium · desktop workspace</p>
            <h2>{current.title}</h2>
            <p className="app-shell__subtitle">{current.subtitle}</p>
          </div>

          <div className="app-shell__pill-row" aria-label="Workspace summary">
            <span className="app-shell__pill">
              <Sparkles size={14} />
              calm control
            </span>
            <span className="app-shell__pill">light-first</span>
            <span className="app-shell__pill">browser-scale ready</span>
          </div>
        </header>

        <div className="app-shell__content">{children}</div>
      </main>
    </div>
  );
}
