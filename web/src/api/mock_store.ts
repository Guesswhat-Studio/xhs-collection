import type { Album, CreateAlbumInput } from '../types/album';
import type { LabelStat } from '../types/label';
import type { BulkNoteActionInput, Note, NoteFilters, UpdateNoteInput } from '../types/note';
import type { StatsSnapshot } from '../types/api';

const today = '2026-04-12';

const noteSeeds: Note[] = [
  {
    id: 'note-tokyo-line',
    title: '东京浅草到上野一日路线，雨天也能走得很顺',
    excerpt: '把浅草寺、河童桥、阿美横町串成一条不绕路的顺走线，适合第一次去东京时快速收一轮。',
    content: '这条路线重点不是打卡点有多少，而是节奏很稳。上午浅草拍照，午后去河童桥挑厨房杂货，傍晚再回到上野逛吃，雨天也不会狼狈。',
    heroLabel: 'Trip route',
    tone: 'travel',
    accountName: 'Rasin · personal',
    publishDate: '2025-11-26',
    fetchedAt: today,
    sourceUrl: 'https://www.xiaohongshu.com/explore/note-tokyo-line',
    rawTags: ['东京', '浅草', '上野', '路线'],
    systemLabels: ['旅行', '路线规划'],
    userLabels: ['待整理', '适合朋友同行'],
    albumIds: ['album-japan'],
    userNote: '适合做成东京轻旅行模板，餐厅和河童桥那段值得单独摘出来。',
    staleness: 'fresh',
    archived: false,
  },
  {
    id: 'note-guangzhou-food',
    title: '广州三家适合带朋友去的馆子，排队也值得',
    excerpt: '不是那种一次性爆红店，胜在稳定、环境松弛，适合外地朋友来玩时直接带。',
    content: '第一家适合午饭，第二家偏夜宵，第三家环境很稳，适合带长辈。每一家都给了点菜建议和避雷项。',
    heroLabel: 'Food map',
    tone: 'food',
    accountName: 'Rasin · personal',
    publishDate: '2024-09-10',
    fetchedAt: today,
    sourceUrl: 'https://www.xiaohongshu.com/explore/note-guangzhou-food',
    rawTags: ['广州', '探店', '朋友聚餐'],
    systemLabels: ['美食', '本地攻略'],
    userLabels: ['周末安排'],
    albumIds: ['album-guangzhou'],
    userNote: '下次可以和“广州 walk”专题放在同一个专辑。',
    staleness: 'aging',
    archived: false,
  },
  {
    id: 'note-skincare-list',
    title: '换季敏感肌维稳清单：早晚只留这 5 样',
    excerpt: '适合状态不稳定时快速缩减步骤，先把皮肤救回来，而不是继续叠加功课。',
    content: '这篇笔记核心价值是取舍。不是全都要，而是先恢复屏障。尤其是喷雾、面霜和酸类的使用频率写得很清楚。',
    heroLabel: 'Reset routine',
    tone: 'beauty',
    accountName: 'Rasin · personal',
    publishDate: '2025-03-14',
    fetchedAt: today,
    sourceUrl: 'https://www.xiaohongshu.com/explore/note-skincare-list',
    rawTags: ['敏感肌', '护肤', '换季'],
    systemLabels: ['护肤', '清单'],
    userLabels: ['想实测'],
    albumIds: ['album-beauty'],
    userNote: '可以和现有在用产品做一版对照。',
    staleness: 'fresh',
    archived: false,
  },
  {
    id: 'note-seoul-hotel',
    title: '首尔住哪一区最省心：明洞、圣水、弘大怎么选',
    excerpt: '不是单纯比热闹，而是从动线、回酒店的体感、购物和咖啡店密度来选。',
    content: '明洞适合第一次去，圣水适合拍照和慢逛，弘大更灵活。作者把每一区的节奏感写得很具体。',
    heroLabel: 'Stay guide',
    tone: 'travel',
    accountName: 'Rasin · studio',
    publishDate: '2023-12-08',
    fetchedAt: today,
    sourceUrl: 'https://www.xiaohongshu.com/explore/note-seoul-hotel',
    rawTags: ['首尔', '酒店', '住宿'],
    systemLabels: ['旅行', '住宿'],
    userLabels: ['待复盘'],
    albumIds: ['album-japan'],
    userNote: '虽然是首尔，但结构和东京住宿比较方式很像。',
    staleness: 'outdated',
    archived: false,
  },
  {
    id: 'note-weekend-walk',
    title: '深圳周末 city walk：咖啡、展览和一个好坐的小公园',
    excerpt: '路线不长，适合周六下午慢慢走，想放松但又不想完全没安排的时候很合适。',
    content: '从咖啡店出发，穿过两条安静街道，最后到一个可以坐很久的小公园。重点是节奏舒服，不是打卡数量。',
    heroLabel: 'Weekend mood',
    tone: 'planning',
    accountName: 'Rasin · personal',
    publishDate: '2025-07-22',
    fetchedAt: today,
    sourceUrl: 'https://www.xiaohongshu.com/explore/note-weekend-walk',
    rawTags: ['深圳', 'citywalk', '周末'],
    systemLabels: ['周末安排', '路线规划'],
    userLabels: ['可复制'],
    albumIds: ['album-weekend'],
    userNote: '很适合做成“低消耗周末”专辑。',
    staleness: 'fresh',
    archived: false,
  },
  {
    id: 'note-flight-tricks',
    title: '廉航托运行李怎么买最不亏，这几个节点别忘了',
    excerpt: '时效性很强，但规则写得很清楚。适合和签证、机场交通一起组成出行准备包。',
    content: '重点在出票后、值机前、机场柜台三个价格节点的差异，还提到了不同平台的隐藏附加费。',
    heroLabel: 'Time-sensitive',
    tone: 'planning',
    accountName: 'Rasin · personal',
    publishDate: '2022-06-18',
    fetchedAt: today,
    sourceUrl: 'https://www.xiaohongshu.com/explore/note-flight-tricks',
    rawTags: ['廉航', '行李', '机票'],
    systemLabels: ['旅行', '出行准备'],
    userLabels: ['需要更新'],
    albumIds: ['album-japan'],
    userNote: '这条过期风险高，保留结构但要重新核对价格规则。',
    staleness: 'outdated',
    archived: false,
  },
  {
    id: 'note-notion-table',
    title: '把购物清单放进 Notion 后，我终于知道哪些东西一直在重复买',
    excerpt: '很适合参考它的字段结构，但视觉风格不想做得像一个纯 Notion 数据库。',
    content: '作者把购买频率、替代品、预算上限都放进去了，重点是结构而不是长篇心得。',
    heroLabel: 'Structure idea',
    tone: 'planning',
    accountName: 'Rasin · studio',
    publishDate: '2025-01-03',
    fetchedAt: today,
    sourceUrl: 'https://www.xiaohongshu.com/explore/note-notion-table',
    rawTags: ['Notion', '清单', '结构'],
    systemLabels: ['结构参考'],
    userLabels: ['借鉴信息架构'],
    albumIds: ['album-weekend'],
    userNote: '保留字段结构灵感，不要继承它那种冷静数据库味。',
    staleness: 'fresh',
    archived: false,
  },
  {
    id: 'note-lip-combo',
    title: '低饱和通勤唇色组合，见客户也不会显得太甜',
    excerpt: '这类内容回看频率高，重点是色调和场景而不是单支产品本身。',
    content: '作者把不同肤色和场景都写进去了，尤其适合做“通勤显气色”专题。',
    heroLabel: 'Shade note',
    tone: 'beauty',
    accountName: 'Rasin · personal',
    publishDate: '2024-12-17',
    fetchedAt: today,
    sourceUrl: 'https://www.xiaohongshu.com/explore/note-lip-combo',
    rawTags: ['口红', '通勤', '低饱和'],
    systemLabels: ['美妆', '穿搭氛围'],
    userLabels: ['精华'],
    albumIds: ['album-beauty'],
    userNote: '可以和穿搭的“低饱和通勤”放到一起。',
    staleness: 'aging',
    archived: false,
  },
  {
    id: 'note-old-archive',
    title: '上海某家老咖啡馆复古陈设合集',
    excerpt: '内容很好看，但已经不想放在日常整理流里了，适合归档保留。',
    content: '更偏灵感收集而非功能信息，所以保留，但不需要一直出现在主视图。',
    heroLabel: 'Archive keep',
    tone: 'food',
    accountName: 'Rasin · studio',
    publishDate: '2023-03-02',
    fetchedAt: today,
    sourceUrl: 'https://www.xiaohongshu.com/explore/note-old-archive',
    rawTags: ['上海', '咖啡馆', '复古'],
    systemLabels: ['灵感', '空间'],
    userLabels: ['旧收藏'],
    albumIds: [],
    userNote: '保留气质参考，但不参与近期整理。',
    staleness: 'aging',
    archived: true,
  },
];

const albumSeeds: Album[] = [
  {
    id: 'album-japan',
    name: '东京 / 首尔行前包',
    description: '路线、住宿、机场准备和要提前核对的事项集中放在一起。',
    tone: 'travel',
    updatedAt: today,
    noteIds: ['note-tokyo-line', 'note-seoul-hotel', 'note-flight-tricks'],
  },
  {
    id: 'album-guangzhou',
    name: '广州带朋友去',
    description: '适合朋友来玩时直接拿来安排吃饭和散步的松弛路线。',
    tone: 'food',
    updatedAt: today,
    noteIds: ['note-guangzhou-food'],
  },
  {
    id: 'album-beauty',
    name: '通勤气色与稳定护肤',
    description: '不追热点，重点是好用、稳、容易回看。',
    tone: 'beauty',
    updatedAt: today,
    noteIds: ['note-skincare-list', 'note-lip-combo'],
  },
  {
    id: 'album-weekend',
    name: '低消耗周末',
    description: '那些不需要太多准备，却能让周末变得很像周末的收藏。',
    tone: 'planning',
    updatedAt: today,
    noteIds: ['note-weekend-walk', 'note-notion-table'],
  },
];

let notesDb = structuredClone(noteSeeds);
let albumsDb = structuredClone(albumSeeds);

function includesSearch(note: Note, value: string) {
  const haystack = [
    note.title,
    note.excerpt,
    note.content,
    note.accountName,
    note.rawTags.join(' '),
    note.systemLabels.join(' '),
    note.userLabels.join(' '),
    note.userNote,
  ]
    .join(' ')
    .toLowerCase();

  return haystack.includes(value.toLowerCase());
}

export function listNotes(filters: NoteFilters): Note[] {
  return notesDb.filter((note) => {
    if (!filters.showArchived && note.archived) {
      return false;
    }

    if (filters.searchText && !includesSearch(note, filters.searchText)) {
      return false;
    }

    if (filters.staleness !== 'all' && note.staleness !== filters.staleness) {
      return false;
    }

    if (filters.accountName !== 'all' && note.accountName !== filters.accountName) {
      return false;
    }

    if (filters.label !== 'all') {
      const labels = [...note.systemLabels, ...note.userLabels];
      if (!labels.includes(filters.label)) {
        return false;
      }
    }

    return true;
  });
}

export function getNote(id: string) {
  return notesDb.find((note) => note.id === id) ?? null;
}

export function updateNote(input: UpdateNoteInput): Note {
  const index = notesDb.findIndex((note) => note.id === input.id);
  if (index === -1) {
    throw new Error('Note not found.');
  }

  const next = {
    ...notesDb[index],
    ...input,
  } satisfies Note;

  notesDb[index] = next;

  albumsDb = albumsDb.map((album) => ({
    ...album,
    noteIds: next.albumIds.includes(album.id)
      ? Array.from(new Set([...album.noteIds.filter((id) => id !== next.id), next.id]))
      : album.noteIds.filter((id) => id !== next.id),
    updatedAt: next.albumIds.includes(album.id) ? today : album.updatedAt,
  }));

  return next;
}

export function applyBulkAction(input: BulkNoteActionInput) {
  input.ids.forEach((id) => {
    const note = getNote(id);
    if (!note) {
      return;
    }

    if (input.action === 'archive') {
      updateNote({ id, archived: true });
      return;
    }

    if (input.action === 'restore') {
      updateNote({ id, archived: false });
      return;
    }

    if (input.action === 'tag-focus') {
      const nextLabels = Array.from(new Set([...note.userLabels, '待复盘']));
      updateNote({ id, userLabels: nextLabels });
      return;
    }

    if (input.action === 'album-weekend') {
      const nextAlbums = Array.from(new Set([...note.albumIds, 'album-weekend']));
      updateNote({ id, albumIds: nextAlbums });
    }
  });

  return notesDb.filter((note) => input.ids.includes(note.id));
}

export function listAlbums(): Album[] {
  return albumsDb.map((album) => ({
    ...album,
    noteIds: notesDb.filter((note) => note.albumIds.includes(album.id)).map((note) => note.id),
  }));
}

export function createAlbum(input: CreateAlbumInput): Album {
  const album: Album = {
    id: `album-${Math.random().toString(36).slice(2, 8)}`,
    name: input.name,
    description: input.description,
    tone: input.tone,
    updatedAt: today,
    noteIds: [],
  };

  albumsDb = [album, ...albumsDb];
  return album;
}

export function listLabels(): LabelStat[] {
  const counter = new Map<string, LabelStat>();

  notesDb.forEach((note) => {
    note.systemLabels.forEach((name) => {
      const existing = counter.get(name);
      counter.set(name, {
        name,
        kind: 'system',
        count: existing ? existing.count + 1 : 1,
      });
    });

    note.userLabels.forEach((name) => {
      const existing = counter.get(name);
      counter.set(name, {
        name,
        kind: 'user',
        count: existing ? existing.count + 1 : 1,
      });
    });
  });

  return [...counter.values()].sort((left, right) => right.count - left.count || left.name.localeCompare(right.name));
}

export function getStats(): StatsSnapshot {
  const activeNotes = notesDb.filter((note) => !note.archived);
  const labels = listLabels();

  const bySystemLabel = Array.from(
    activeNotes
      .flatMap((note) => note.systemLabels)
      .reduce((map, name) => map.set(name, (map.get(name) ?? 0) + 1), new Map<string, number>()),
  )
    .map(([label, count]) => ({ label, count }))
    .sort((left, right) => right.count - left.count)
    .slice(0, 6);

  const byAccount = Array.from(
    activeNotes.reduce((map, note) => map.set(note.accountName, (map.get(note.accountName) ?? 0) + 1), new Map<string, number>()),
  ).map(([label, count]) => ({ label, count }));

  return {
    totalNotes: notesDb.length,
    activeNotes: activeNotes.length,
    archivedNotes: notesDb.length - activeNotes.length,
    albumsCount: albumsDb.length,
    labelCount: labels.length,
    freshCount: activeNotes.filter((note) => note.staleness === 'fresh').length,
    agingCount: activeNotes.filter((note) => note.staleness === 'aging').length,
    outdatedCount: activeNotes.filter((note) => note.staleness === 'outdated').length,
    lastSyncedAt: today,
    bySystemLabel,
    byAccount,
  };
}

export function getAlbumNotes(albumId: string) {
  return notesDb.filter((note) => note.albumIds.includes(albumId));
}

export function getAccounts() {
  return Array.from(new Set(notesDb.map((note) => note.accountName)));
}

export function getSuggestedLabels() {
  return listLabels()
    .filter((label) => label.kind === 'user')
    .slice(0, 8)
    .map((label) => label.name);
}
