pub(crate) const XHS_UNWRAP_JS: &str = r#"
function unwrap(obj, depth) {
  if (depth > 6 || obj === null || obj === undefined) return obj;
  if (typeof obj !== 'object') return obj;
  if ('_value' in obj && 'dep' in obj) return unwrap(obj._value, depth + 1);
  if ('value' in obj && 'dep' in obj) return unwrap(obj.value, depth + 1);
  if (Array.isArray(obj)) return obj.map(item => unwrap(item, depth + 1));
  const result = {};
  for (const key of Object.keys(obj)) {
    if (key === 'dep' || key.startsWith('__')) continue;
    try { result[key] = unwrap(obj[key], depth + 1); } catch (_) {}
  }
  return result;
}
"#;

pub(crate) const XHS_SELF_INFO_SCRIPT: &str = r#"
(() => {
__UNWRAP_JS__
  const state = window.__INITIAL_STATE__;
  if (!state) return { ready: false };

  const candidates = [
    state.user && state.user.userInfo,
    state.user && state.user.currentUser,
    state.user && state.user.loginUser,
    state.user && state.user.info,
    state.user,
    state.sidebar && state.sidebar.user,
    state.app && state.app.user,
    state.user && state.user.userPageData
  ];

  function pickUser(data) {
    if (!data || typeof data !== 'object') return null;
    const basic = data.basicInfo || data.basic_info || data.userInfo || data.user_info || data;
    const userPageBasic = data.userPageData && (data.userPageData.basicInfo || data.userPageData.basic_info);
    const source = userPageBasic || basic;
    const userId = source.userId || source.user_id || source.id || data.userId || data.user_id || data.id || '';
    const nickname = source.nickname || source.nick_name || source.name || data.nickname || data.nick_name || '';
    const avatar = source.avatar || source.image || source.avatarUrl || source.avatar_url || source.images || data.avatar || data.avatarUrl || data.avatar_url || '';
    if (userId || nickname) return { userId: String(userId || ''), nickname: String(nickname || ''), avatarUrl: String(avatar || '') };
    return null;
  }

  for (const candidate of candidates) {
    const picked = pickUser(unwrap(candidate, 0));
    if (picked) return { ready: true, ...picked };
  }

  return { ready: true, userId: '', nickname: '' };
})()
"#;

pub(crate) const XHS_FAVORITES_SCRIPT: &str = r#"
(() => {
__UNWRAP_JS__
  const debug = { href: window.location.href, title: document.title || '', userKeys: [], candidatePaths: [], linkCount: 0, cardCount: 0 };

  function get(obj, path) {
    let cur = obj;
    for (const key of path) {
      if (cur === null || cur === undefined) return undefined;
      cur = cur[key];
    }
    return cur;
  }

  function first(obj, paths) {
    for (const path of paths) {
      const value = get(obj, path);
      if (value !== null && value !== undefined && String(value).trim()) return String(value).trim();
    }
    return '';
  }

  function noteIdOf(item) {
    return first(item, [
      ['noteId'], ['note_id'], ['id'],
      ['note', 'noteId'], ['note', 'note_id'], ['note', 'id'],
      ['noteCard', 'noteId'], ['noteCard', 'note_id'], ['noteCard', 'id'],
      ['note_card', 'noteId'], ['note_card', 'note_id'], ['note_card', 'id'],
    ]);
  }

  function noteScore(item) {
    if (!item || typeof item !== 'object') return 0;
    const id = noteIdOf(item);
    if (!id || id.length < 8) return 0;
    let score = 1;
    if (first(item, [['displayTitle'], ['display_title'], ['title'], ['noteCard', 'displayTitle'], ['note_card', 'display_title']])) score += 1;
    if (first(item, [['cover', 'url'], ['noteCard', 'cover', 'url'], ['note_card', 'cover', 'url']])) score += 1;
    return score;
  }

  function arrayItems(value) {
    const data = unwrap(value, 0);
    if (Array.isArray(data)) return data;
    if (data && typeof data === 'object') {
      for (const key of ['value', '_value', 'data', 'list', 'items', 'feeds', 'notes']) {
        if (Array.isArray(data[key])) return data[key];
      }
    }
    return [];
  }

  function collectArrays(obj, path, depth, out, seen) {
    if (!obj || depth > 6) return;
    const data = unwrap(obj, 0);
    if (!data || typeof data !== 'object') return;
    if (seen.has(data)) return;
    seen.add(data);
    if (Array.isArray(data)) {
      const scored = data
        .filter(item => item && typeof item === 'object')
        .map(item => ({ item, score: noteScore(item) }))
        .filter(entry => entry.score > 0);
      if (scored.length) {
        out.push({ path, score: scored.reduce((sum, entry) => sum + entry.score, 0), items: scored.map(entry => entry.item) });
      }
      return;
    }
    const keys = Object.keys(data).sort((a, b) => {
      const rank = key => /fav|collect|collection|note|feed|list|item/i.test(key) ? 0 : 1;
      return rank(a) - rank(b);
    });
    for (const key of keys) {
      if (key === 'dep' || key.startsWith('__')) continue;
      collectArrays(data[key], path ? `${path}.${key}` : key, depth + 1, out, seen);
    }
  }

  function notesFromState() {
    const state = window.__INITIAL_STATE__;
    if (!state) return [];
    if (state.user && typeof state.user === 'object') debug.userKeys = Object.keys(unwrap(state.user, 0)).slice(0, 40);
    const u = state.user || {};
    const sources = [
      u.collect, u.collectNotes, u.collection, u.collections, u.favorite, u.favorites,
      u.userCollectNotes, u.favNotes, u.notes, u.feeds, u.profile && u.profile.notes,
      u.userPageData && u.userPageData.notes,
    ];
    for (const src of sources) {
      const items = arrayItems(src);
      if (items.some(item => noteScore(item) > 0)) return items;
    }
    const candidates = [];
    collectArrays(state.user, 'user', 0, candidates, new WeakSet());
    candidates.sort((a, b) => b.score - a.score);
    debug.candidatePaths = candidates.slice(0, 8).map(candidate => `${candidate.path}:${candidate.items.length}`);
    return candidates[0] ? candidates[0].items : [];
  }

  function notesFromDom() {
    const cards = document.querySelectorAll('section.note-item, [class*="note-item"], [class*="note-card"], a[href*="/explore/"], a[href*="/discovery/item/"], a[href*="/user/profile/"]');
    debug.cardCount = cards.length;
    debug.linkCount = document.querySelectorAll('a[href*="/explore/"], a[href*="/discovery/item/"]').length;
    return Array.from(cards).map(card => {
      const links = (card.matches && (card.matches('a[href*="/explore/"]') || card.matches('a[href*="/discovery/item/"]') || card.matches('a[href*="/user/profile/"]')))
        ? [card]
        : Array.from(card.querySelectorAll('a[href*="/explore/"], a[href*="/discovery/item/"], a[href*="/user/profile/"]'));
      const noteIdFromHref = href => {
        const text = String(href || '');
        return (text.match(/\/(?:explore|discovery\/item)\/([a-zA-Z0-9]+)/) || [])[1]
          || (text.match(/\/user\/profile\/[a-zA-Z0-9]+\/([a-zA-Z0-9]+)/) || [])[1]
          || '';
      };
      const noteLinks = links.filter(link => noteIdFromHref(link.getAttribute('href') || link.href || ''));
      const link = noteLinks.find(link => (link.href || link.getAttribute('href') || '').includes('xsec_token='))
        || noteLinks.find(link => (link.href || link.getAttribute('href') || '').includes('/user/profile/'))
        || noteLinks[0]
        || null;
      const href = link ? (link.getAttribute('href') || link.href || '') : '';
      const sourceUrl = href ? new URL(href, window.location.origin).href : '';
      let xsecToken = '';
      let xsecSource = '';
      try {
        const parsed = sourceUrl ? new URL(sourceUrl) : null;
        xsecToken = parsed ? (parsed.searchParams.get('xsec_token') || '') : '';
        xsecSource = parsed ? (parsed.searchParams.get('xsec_source') || '') : '';
      } catch (_) {}
      const titleEl = card.querySelector('[class*="title"]');
      const authorEl = card.querySelector('[class*="author"], [class*="name"], [class*="user"]');
      const img = card.querySelector('img');
      return {
        noteId: noteIdFromHref(href),
        displayTitle: titleEl ? titleEl.textContent.trim() : '',
        user: { nickname: authorEl ? authorEl.textContent.trim() : '' },
        cover: { url: img ? (img.currentSrc || img.src || '') : '' },
        sourceUrl,
        xsecToken,
        xsecSource: xsecSource || 'pc_collect',
      };
    });
  }

  const stateNotes = notesFromState();
  const domNotes = notesFromDom();
  const seen = new Set();
  const notes = [];
  for (const note of [...stateNotes, ...domNotes]) {
    const noteId = noteIdOf(note);
    if (!noteId || seen.has(noteId)) continue;
    seen.add(noteId);
    notes.push(note);
  }
  window.__XHS_COLLECTION_DEBUG__ = {
    ...debug,
    returned: notes.length,
    stateCount: stateNotes.length,
    domCount: domNotes.length,
    scrollY: window.scrollY || document.documentElement.scrollTop || 0,
    innerHeight: window.innerHeight || 0,
    scrollHeight: document.documentElement.scrollHeight || document.body.scrollHeight || 0,
    atBottom: (window.scrollY || document.documentElement.scrollTop || 0) + (window.innerHeight || 0) >= ((document.documentElement.scrollHeight || document.body.scrollHeight || 0) - 12),
  };
  return notes;
})()
"#;

pub(crate) const XHS_ALBUMS_SCRIPT: &str = r#"
(() => {
__UNWRAP_JS__
  const debug = { href: window.location.href, title: document.title || '', stateCandidates: 0, domCandidates: 0 };

  function get(obj, path) {
    let cur = obj;
    for (const key of path) {
      if (cur === null || cur === undefined) return undefined;
      cur = cur[key];
    }
    return cur;
  }

  function first(obj, paths) {
    for (const path of paths) {
      const value = get(obj, path);
      if (value !== null && value !== undefined && String(value).trim()) return String(value).trim();
    }
    return '';
  }

  function num(obj, paths) {
    for (const path of paths) {
      const value = get(obj, path);
      const raw = String(value ?? '');
      if (!/\d/.test(raw)) continue;
      const parsed = Number(raw.replace(/[^\d]/g, ''));
      if (Number.isFinite(parsed) && parsed >= 0) return parsed;
    }
    return null;
  }

  function albumIdOf(item) {
    return first(item, [
      ['albumId'], ['album_id'], ['boardId'], ['board_id'], ['collectionId'], ['collection_id'],
      ['collectId'], ['collect_id'], ['favId'], ['fav_id'],
      ['id'], ['album', 'id'], ['board', 'id'], ['collection', 'id']
    ]);
  }

  function albumNameOf(item) {
    return first(item, [
      ['name'], ['title'], ['displayTitle'], ['display_title'],
      ['albumName'], ['album_name'], ['boardName'], ['board_name'], ['collectionName'], ['collection_name'],
      ['album', 'name'], ['board', 'name'], ['collection', 'name']
    ]);
  }

  function albumUrlOf(item) {
    return first(item, [
      ['sourceUrl'], ['source_url'], ['url'], ['link'], ['href'],
      ['albumUrl'], ['album_url'], ['boardUrl'], ['board_url'], ['collectionUrl'], ['collection_url'],
    ]);
  }

  function noteIdOf(item) {
    return first(item, [
      ['noteId'], ['note_id'],
      ['note', 'noteId'], ['note', 'note_id'], ['note', 'id'],
      ['noteCard', 'noteId'], ['noteCard', 'note_id'], ['noteCard', 'id'],
      ['note_card', 'noteId'], ['note_card', 'note_id'], ['note_card', 'id']
    ]);
  }

  function cleanAlbumUrl(raw) {
    const text = String(raw || '').trim();
    if (!text || text === '#' || /^javascript:/i.test(text)) return '';
    let url = '';
    try {
      url = new URL(text, window.location.origin).href;
    } catch (_) {
      return '';
    }
    const lower = url.toLowerCase();
    if (!lower.startsWith('https://www.xiaohongshu.com/') && !lower.startsWith('https://xiaohongshu.com/')) return '';
    if (lower.includes('beian.miit.gov.cn') || lower.includes('/404') || lower.includes('source=404')) return '';
    if (lower.includes('/explore') || lower.includes('/discovery/item')) return '';
    if (lower.includes('subtab=note') || lower.includes('subtab=file')) return '';
    if (/(?:\/|[?&=_-])file(?:\/|=|&|$)|fileid|file_id/.test(lower)) return '';
    const hasDetailId = /albumid|album_id|boardid|board_id|collectionid|collection_id/.test(lower);
    const hasDetailPath = /\/(?:album|board|collection)\//.test(lower);
    if (lower.includes('/user/profile/') && !hasDetailId) return '';
    if (!hasDetailId && !hasDetailPath) return '';
    return url.replace('https://xiaohongshu.com/', 'https://www.xiaohongshu.com/');
  }

  function albumScore(item) {
    if (!item || typeof item !== 'object') return 0;
    if (noteIdOf(item)) return 0;
    const id = albumIdOf(item);
    const name = albumNameOf(item);
    const url = cleanAlbumUrl(albumUrlOf(item));
    const keys = Object.keys(item).join('|');
    const hasAlbumKey = /album|board|collection|albumName|boardName|collectionName|noteCount|itemCount|itemsCount/i.test(keys);
    if (!hasAlbumKey && !url) return 0;
    let score = 0;
    if (id && id.length >= 4) score += 2;
    if (name && name.length >= 1) score += 2;
    if (url) score += 2;
    if (hasAlbumKey) score += 2;
    return score >= 3 ? score : 0;
  }

  function collectAlbums(obj, path, depth, out, seen) {
    if (!obj || depth > 7) return;
    const data = unwrap(obj, 0);
    if (!data || typeof data !== 'object' || seen.has(data)) return;
    seen.add(data);
    if (Array.isArray(data)) {
      for (const item of data) {
        if (albumScore(item) > 0) out.push({ path, item });
      }
      return;
    }
    const keys = Object.keys(data).sort((a, b) => {
      const rank = key => /album|collection|collect|folder|file|fav|list|item/i.test(key) ? 0 : 1;
      return rank(a) - rank(b);
    });
    for (const key of keys) {
      if (key === 'dep' || key.startsWith('__')) continue;
      collectAlbums(data[key], path ? `${path}.${key}` : key, depth + 1, out, seen);
    }
  }

  function albumsFromState() {
    const state = window.__INITIAL_STATE__;
    if (!state) return [];
    const candidates = [];
    collectAlbums(state, 'state', 0, candidates, new WeakSet());
    debug.stateCandidates = candidates.length;
    return candidates.map(entry => entry.item);
  }

  function albumsFromDom() {
    const elements = Array.from(document.querySelectorAll([
      'a[href]',
      '[role="button"]',
      '[class*="album"]',
      '[class*="board"]',
      '[class*="collection"]',
    ].join(',')));
    const candidates = elements
      .filter(element => {
        const anchor = element.matches && element.matches('a[href]') ? element : element.closest('a[href]');
        const href = (element.getAttribute('href') || (anchor && anchor.getAttribute('href')) || '');
        const text = (element.textContent || '').trim();
        return cleanAlbumUrl(href) || /专辑|收藏夹/.test(text);
      })
      .map(element => {
        const anchor = element.matches && element.matches('a[href]') ? element : element.closest('a[href]');
        const href = (element.getAttribute('href') || (anchor && anchor.getAttribute('href')) || '');
        const sourceUrl = cleanAlbumUrl(href);
        const text = (element.textContent || '').replace(/\s+/g, ' ').trim();
        const img = element.querySelector('img') || (anchor && anchor.querySelector('img'));
        const dataId =
          element.getAttribute('data-id') ||
          element.getAttribute('data-album-id') ||
          element.getAttribute('data-board-id') ||
          element.getAttribute('data-collection-id') ||
          '';
        const id =
          (sourceUrl.match(/\/album(?:\/|=)([a-zA-Z0-9_-]+)/) || [])[1] ||
          (sourceUrl.match(/\/board(?:\/|=)([a-zA-Z0-9_-]+)/) || [])[1] ||
          (sourceUrl.match(/\/collection(?:\/|=)([a-zA-Z0-9_-]+)/) || [])[1] ||
          (sourceUrl.match(/\/fav(?:\/|=)([a-zA-Z0-9_-]+)/) || [])[1] ||
          (sourceUrl.match(/[?&](?:albumId|album_id|boardId|board_id|collectionId|collection_id|id)=([a-zA-Z0-9_-]+)/) || [])[1] ||
          dataId ||
          '';
        return {
          albumId: id || sourceUrl,
          name: text || element.getAttribute('title') || (anchor && anchor.getAttribute('title')) || '未命名专辑',
          sourceUrl,
          coverUrl: img ? (img.currentSrc || img.src || '') : '',
          noteCount: num({ text }, [['text']])
        };
      });
    debug.domCandidates = candidates.length;
    return candidates;
  }

  const seen = new Set();
  const albums = [];
  for (const raw of [...albumsFromState(), ...albumsFromDom()]) {
    const sourceUrl = cleanAlbumUrl(albumUrlOf(raw));
    const id = albumIdOf(raw) || sourceUrl;
    const name = albumNameOf(raw);
    if (!id || !name) continue;
    const key = `${id}|${name}`;
    if (seen.has(key)) continue;
    seen.add(key);
    albums.push({
      albumId: id,
      name,
      description: first(raw, [['desc'], ['description'], ['intro']]),
      sourceUrl,
      coverUrl: first(raw, [['cover', 'url'], ['coverUrl'], ['cover_url'], ['image', 'url']]),
      noteCount: num(raw, [['noteCount'], ['note_count'], ['count'], ['total'], ['itemsCount'], ['item_count']]),
      raw
    });
  }
  window.__XHS_COLLECTION_DEBUG__ = {
    ...debug,
    returned: albums.length,
    scrollY: window.scrollY || document.documentElement.scrollTop || 0,
    innerHeight: window.innerHeight || 0,
    scrollHeight: document.documentElement.scrollHeight || document.body.scrollHeight || 0,
    atBottom: (window.scrollY || document.documentElement.scrollTop || 0) + (window.innerHeight || 0) >= ((document.documentElement.scrollHeight || document.body.scrollHeight || 0) - 12),
  };
  return albums;
})()
"#;

pub(crate) const XHS_CLICK_COLLECTION_TAB_SCRIPT: &str = r#"
(() => {
  const labels = __TARGET_LABELS__;
  const debug = { href: window.location.href, labels, clicked: false, text: '', element: '', href: '' };

  function visible(el) {
    const rect = el.getBoundingClientRect();
    const style = window.getComputedStyle(el);
    return rect.width > 0 && rect.height > 0 && style.display !== 'none' && style.visibility !== 'hidden';
  }

  function norm(value) {
    return String(value || '')
      .replace(/\s+/g, '')
      .replace(/[0-9０-９]+$/, '')
      .trim();
  }

  function labelText(el) {
    return norm(
      el.innerText ||
      el.textContent ||
      el.getAttribute('aria-label') ||
      el.getAttribute('title') ||
      ''
    );
  }

  const selector = [
    'a[href]',
    'button',
    '[role="tab"]',
    '[role="button"]',
    '[class*="tab"]',
    '[class*="nav"]',
    '[class*="collect"]',
    '[class*="fav"]',
    '[class*="album"]',
    '[class*="folder"]',
    '[class*="file"]'
  ].join(',');
  const targets = labels.map(norm).filter(Boolean);
  const candidates = [];
  for (const el of Array.from(document.querySelectorAll(selector))) {
    if (!visible(el)) continue;
    const text = labelText(el);
    if (!text) continue;
    for (const target of targets) {
      const exact = text === target;
      if (exact || text.includes(target)) {
        candidates.push({
          el,
          text,
          exact,
          href: el.getAttribute('href') || '',
          score: (exact ? 4 : 0) - Math.max(0, text.length - target.length)
        });
      }
    }
  }
  candidates.sort((a, b) => b.score - a.score || a.text.length - b.text.length);
  const picked = candidates[0];
  if (!picked) return debug;
  picked.el.scrollIntoView({ block: 'center', inline: 'center' });
  picked.el.click();
  window.scrollTo(0, 0);
  return {
    ...debug,
    clicked: true,
    text: picked.text,
    element: picked.el.tagName || '',
    href: picked.href
  };
})()
"#;

pub(crate) const XHS_FAVORITES_DEBUG_SCRIPT: &str = r#"
(() => window.__XHS_COLLECTION_DEBUG__ || {
  href: window.location.href,
  title: document.title || '',
  userKeys: [],
  candidatePaths: [],
  linkCount: document.querySelectorAll('a[href*="/explore/"], a[href*="/discovery/item/"]').length,
  cardCount: document.querySelectorAll('section.note-item, [class*="note-item"], [class*="note-card"]').length,
  stateCount: 0,
  domCount: 0,
  scrollY: window.scrollY || document.documentElement.scrollTop || 0,
  innerHeight: window.innerHeight || 0,
  scrollHeight: document.documentElement.scrollHeight || document.body.scrollHeight || 0,
  atBottom: (window.scrollY || document.documentElement.scrollTop || 0) + (window.innerHeight || 0) >= ((document.documentElement.scrollHeight || document.body.scrollHeight || 0) - 12),
  returned: 0
})()
"#;

pub(crate) const XHS_FAVORITES_API_HOOK_INSTALL_SCRIPT: &str = r#"
(() => {
  if (window.__XHS_COLLECTION_API_HOOK__ && window.__XHS_COLLECTION_API_HOOK__.installed) {
    return { installed: true, alreadyInstalled: true };
  }

  const hook = { installed: true, pages: [], errors: [] };
  window.__XHS_COLLECTION_API_HOOK__ = hook;

  function noteIdOf(note) {
    if (!note || typeof note !== 'object') return '';
    return String(note.note_id || note.noteId || note.id || (note.note_card && (note.note_card.note_id || note.note_card.id)) || '').trim();
  }

  function record(url, payload) {
    try {
      const textUrl = String(url || '');
      if (!textUrl.includes('/api/sns/web/v2/note/collect/page')) return;
      const data = payload && payload.data ? payload.data : payload;
      const notes = data && Array.isArray(data.notes) ? data.notes : [];
      if (!notes.length) return;
      hook.pages.push({
        url: textUrl,
        cursor: data.cursor || '',
        hasMore: Boolean(data.has_more),
        notes,
        first: noteIdOf(notes[0]),
        last: noteIdOf(notes[notes.length - 1]),
        capturedAt: Date.now(),
      });
      if (hook.pages.length > 200) hook.pages.splice(0, hook.pages.length - 200);
    } catch (error) {
      hook.errors.push(`record:${error && error.message ? error.message : String(error)}`);
    }
  }

  try {
    const originalFetch = window.fetch;
    if (originalFetch && !originalFetch.__xhsCollectionHooked) {
      const wrappedFetch = async function(...args) {
        const response = await originalFetch.apply(this, args);
        try {
          const url = typeof args[0] === 'string'
            ? args[0]
            : (args[0] && args[0].url) || '';
          if (String(url).includes('/api/sns/web/v2/note/collect/page')) {
            response.clone().json()
              .then(json => record(url, json))
              .catch(error => hook.errors.push(`fetch:${error && error.message ? error.message : String(error)}`));
          }
        } catch (error) {
          hook.errors.push(`fetch-wrap:${error && error.message ? error.message : String(error)}`);
        }
        return response;
      };
      wrappedFetch.__xhsCollectionHooked = true;
      window.fetch = wrappedFetch;
    }
  } catch (error) {
    hook.errors.push(`fetch-install:${error && error.message ? error.message : String(error)}`);
  }

  try {
    const originalOpen = XMLHttpRequest.prototype.open;
    const originalSend = XMLHttpRequest.prototype.send;
    if (!XMLHttpRequest.prototype.__xhsCollectionHooked) {
      XMLHttpRequest.prototype.open = function(method, url, ...rest) {
        this.__xhsCollectionUrl = url;
        return originalOpen.call(this, method, url, ...rest);
      };
      XMLHttpRequest.prototype.send = function(...args) {
        this.addEventListener('load', function() {
          try {
            const url = this.__xhsCollectionUrl || '';
            if (String(url).includes('/api/sns/web/v2/note/collect/page')) {
              record(url, JSON.parse(this.responseText || '{}'));
            }
          } catch (error) {
            hook.errors.push(`xhr:${error && error.message ? error.message : String(error)}`);
          }
        });
        return originalSend.apply(this, args);
      };
      XMLHttpRequest.prototype.__xhsCollectionHooked = true;
    }
  } catch (error) {
    hook.errors.push(`xhr-install:${error && error.message ? error.message : String(error)}`);
  }

  return { installed: true, alreadyInstalled: false };
})()
"#;

pub(crate) const XHS_FAVORITES_API_HOOK_DRAIN_SCRIPT: &str = r#"
(() => {
  const hook = window.__XHS_COLLECTION_API_HOOK__;
  const scrollY = window.scrollY || document.documentElement.scrollTop || 0;
  const scrollHeight = document.documentElement.scrollHeight || document.body.scrollHeight || 0;
  const atBottom = scrollY + (window.innerHeight || 0) >= (scrollHeight - 12);
  if (!hook) {
    return { installed: false, pages: [], errors: ['not_installed'], scrollY, scrollHeight, atBottom };
  }
  const pages = hook.pages.splice(0, hook.pages.length);
  const errors = hook.errors.splice(0, hook.errors.length);
  return { installed: true, pages, errors, scrollY, scrollHeight, atBottom };
})()
"#;

pub(crate) const XHS_FAVORITES_DISPLAY_COUNT_SCRIPT: &str = r#"
(() => {
  const texts = Array.from(document.querySelectorAll('[class*="tab"], [class*="feeds"], [role="tab"]'))
    .map(el => el.textContent || '')
    .join('\n');
  const bodyText = document.body ? document.body.innerText.slice(0, 1200) : '';
  const match = `${texts}\n${bodyText}`.match(/笔记\s*[・·•]\s*([\d,，]+)/);
  if (!match) return null;
  const count = Number(String(match[1]).replace(/[^\d]/g, ''));
  return Number.isFinite(count) && count > 0 ? count : null;
})()
"#;
