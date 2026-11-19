export const SORT_OPTIONS = ['created_asc', 'created_desc', 'name_asc'];

export function normalize(value) {
  return String(value ?? '').trim().toLocaleLowerCase();
}

export function apiBaseFromPath(path) {
  const clean = String(path || '').replace(/\/+$/, '');
  return clean.replace(/^\/u\//, '/api/');
}

export function usernameFromPath(path) {
  const parts = String(path || '').split('/').filter(Boolean);
  return parts[0] === 'u' && parts.length >= 2 ? parts[1] : '';
}

export function groupItems(items) {
  const groups = new Map();
  for (const item of items || []) {
    const key = normalize(item.category);
    if (!groups.has(key)) {
      groups.set(key, { key, label: item.category, items: [] });
    }
    groups.get(key).items.push(item);
  }
  return [...groups.values()].sort((a, b) =>
    a.label.localeCompare(b.label, undefined, { sensitivity: 'base' })
  );
}

export function compareItems(a, b, sort = 'created_asc') {
  if (a.disabled !== b.disabled) {
    return a.disabled ? 1 : -1;
  }
  if (sort === 'created_desc') {
    return b.id - a.id;
  }
  if (sort === 'name_asc') {
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
  }
  return a.id - b.id;
}

export function visibleGroups(items, settings = {}) {
  const { sort = 'created_asc', category = '', hideDisabled = false } = settings;
  const selected = normalize(category);

  return groupItems(items)
    .filter((group) => !selected || group.key === selected)
    .map((group) => ({
      ...group,
      items: group.items
        .filter((item) => !hideDisabled || !item.disabled)
        .sort((a, b) => compareItems(a, b, sort)),
    }))
    .filter((group) => group.items.length > 0);
}

export function uniqueSuggestions(items, field) {
  const seen = new Set();
  const result = [];
  for (const item of items || []) {
    const value = String(item[field] ?? '').trim();
    const key = normalize(value);
    if (!key || seen.has(key)) continue;
    seen.add(key);
    result.push(value);
  }
  return result.sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }));
}

export function parseSettings(raw) {
  let parsed = {};
  if (typeof raw === 'string') {
    try {
      parsed = JSON.parse(raw);
    } catch {
      parsed = {};
    }
  } else if (raw && typeof raw === 'object') {
    parsed = raw;
  }
  return {
    sort: SORT_OPTIONS.includes(parsed.sort) ? parsed.sort : 'created_asc',
    category: typeof parsed.category === 'string' ? parsed.category : '',
    hideDisabled: parsed.hideDisabled === true,
  };
}

export function serializeSettings(settings) {
  const clean = parseSettings(settings);
  return JSON.stringify(clean);
}

export function cookieString(settings, isHttps = false) {
  const value = encodeURIComponent(serializeSettings(settings));
  let cookie = `2buy_view=${value}; Path=/; SameSite=Lax`;
  if (isHttps) cookie += '; Secure';
  return cookie;
}

export function readCookieValue(cookieHeader, name) {
  const pairs = String(cookieHeader || '').split(';');
  for (const pair of pairs) {
    const [rawKey, ...rest] = pair.trim().split('=');
    if (rawKey === name) {
      return decodeURIComponent(rest.join('='));
    }
  }
  return '';
}
