import type { ReactNode } from 'react';
import { Icon } from './icons';
import type { LibraryOverview, LocalProfileSummary } from '../types/library';

export function ProfileSwitcher({
  overview,
  onSwitchProfile,
}: {
  overview: LibraryOverview | null;
  onSwitchProfile: (profileId: string) => Promise<void> | void;
}) {
  const profiles = overview?.profiles ?? [];
  if (profiles.length === 0) return null;
  return (
    <div className="panel">
      <div className="panel-head">
        <Icon name="user" size={16} />
        <h3>本地账号</h3>
        <span className="count">{profiles.length}</span>
      </div>
      <div className="profile-list">
        {profiles.map((profile) => (
          <button
            className={`profile-row ${profile.isActive ? 'active' : ''}`}
            disabled={profile.isActive}
            key={profile.id}
            onClick={() => onSwitchProfile(profile.id)}
            type="button"
          >
            <ProfileAvatar profile={profile} />
            <span className="profile-main">
              <strong>{profile.displayName || '本机账号'}</strong>
              <span>{profile.sourceAccountId ? `xhs:${profile.sourceAccountId}` : '未绑定小红书'}</span>
            </span>
            {profile.isActive ? <Icon name="checkCircle" size={15} /> : <Icon name="chevronRight" size={15} />}
          </button>
        ))}
      </div>
    </div>
  );
}

export function ProfileGate({
  overview,
  onSelect,
  onClose,
  onConnectNew,
}: {
  overview: LibraryOverview;
  onSelect: (profileId: string) => Promise<void> | void;
  onClose: () => void;
  onConnectNew: () => void;
}) {
  return (
    <div className="profile-gate" role="dialog" aria-modal="true" aria-label="选择本地账号">
      <div className="profile-gate-panel">
        <div className="profile-gate-head">
          <span className="acct-avatar sync-head-icon">
            <Icon name="user" size={16} style={{ color: '#fff' }} />
          </span>
          <div>
            <h2>选择本地账号</h2>
            <p>每个本地账号绑定一个小红书账号，并使用独立 SQLite 与媒体目录。</p>
          </div>
        </div>
        <div className="profile-gate-list">
          {overview.profiles.map((profile) => (
            <button
              className={`profile-gate-row ${profile.isActive ? 'active' : ''}`}
              key={profile.id}
              onClick={() => onSelect(profile.id)}
              type="button"
            >
              <ProfileAvatar profile={profile} />
              <span className="profile-main">
                <strong>{profile.displayName}</strong>
                <span>{profile.sourceAccountId ? `xhs:${profile.sourceAccountId}` : '未绑定小红书'}</span>
              </span>
              {profile.isActive && <span className="conn-status">当前</span>}
            </button>
          ))}
        </div>
        <div className="profile-gate-actions">
          <button className="btn btn-ghost" onClick={onClose} type="button">
            <Icon name="check" size={15} />
            继续使用当前
          </button>
          <button className="btn btn-primary" onClick={onConnectNew} type="button">
            <Icon name="login" size={15} />
            连接新账号
          </button>
        </div>
      </div>
    </div>
  );
}

export function ProfileAvatar({ profile }: { profile: LocalProfileSummary }) {
  return (
    <span className="profile-avatar">
      {profile.avatarUrl ? <img alt="" src={profile.avatarUrl} /> : (profile.displayName || '本').slice(0, 1)}
    </span>
  );
}

export function Step({
  n,
  state,
  title,
  children,
  last = false,
}: {
  n: number;
  state?: 'done' | 'active' | '';
  title: string;
  children: ReactNode;
  last?: boolean;
}) {
  return (
    <div className={`step ${state ?? ''}`}>
      <div className="step-rail">
        <div className="step-num">{state === 'done' ? <Icon name="check" size={16} stroke={2.6} /> : n}</div>
        {!last && <div className="step-line" />}
      </div>
      <div className="step-body">
        <h3>{title}</h3>
        {children}
      </div>
    </div>
  );
}
