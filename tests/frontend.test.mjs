import test from 'node:test';
import assert from 'node:assert/strict';

import {
  apiBaseFromPath,
  cookieString,
  groupItems,
  normalize,
  parseSettings,
  readCookieValue,
  serializeSettings,
  uniqueSuggestions,
  usernameFromPath,
  visibleGroups,
} from '../static/logic.mjs';

const items = [
  { id: 1, name: 'Screwdriver', quantity: '2', category: 'Hardware', comment: '', disabled: false },
  { id: 2, name: 'Milk', quantity: '1', category: 'Dairy', comment: '', disabled: false },
  { id: 3, name: 'Butter', quantity: '1', category: 'dairy', comment: '', disabled: true },
  { id: 4, name: 'HP ink', quantity: '1', category: 'HARDWARE', comment: '', disabled: true },
  { id: 5, name: 'Apples', quantity: '5', category: 'Dairy', comment: '', disabled: false },
];

test('normalize trims and lowercases for all matching', () => {
  assert.equal(normalize('  Hardware Store '), 'hardware store');
  assert.equal(normalize('MiLk'), 'milk');
});

test('groups are case-insensitive and use first-created label', () => {
  const groups = groupItems(items);
  assert.deepEqual(groups.map((group) => group.key), ['dairy', 'hardware']);
  assert.equal(groups[0].label, 'Dairy');
  assert.equal(groups[1].label, 'Hardware');
  assert.deepEqual(groups[0].items.map((item) => item.id), [2, 3, 5]);
  assert.deepEqual(groups[1].items.map((item) => item.id), [1, 4]);
});

test('disabled items go last in every sort mode', () => {
  const asc = visibleGroups(items, { sort: 'created_asc' });
  assert.deepEqual(asc[0].items.map((item) => item.id), [2, 5, 3]);

  const desc = visibleGroups(items, { sort: 'created_desc' });
  assert.deepEqual(desc[0].items.map((item) => item.id), [5, 2, 3]);

  const byName = visibleGroups(items, { sort: 'name_asc' });
  assert.deepEqual(byName[0].items.map((item) => item.id), [5, 2, 3]);
});

test('category filter and hide-disabled are view-only', () => {
  const filtered = visibleGroups(items, { category: 'HARDWARE' });
  assert.deepEqual(filtered.map((group) => group.key), ['hardware']);
  assert.deepEqual(filtered[0].items.map((item) => item.id), [1, 4]);

  const hidden = visibleGroups(items, { hideDisabled: true });
  assert.deepEqual(hidden.map((group) => group.key), ['dairy', 'hardware']);
  assert.deepEqual(hidden[0].items.map((item) => item.id), [2, 5]);

  const both = visibleGroups(items, { category: 'hardware', hideDisabled: true });
  assert.deepEqual(both.flatMap((group) => group.items.map((item) => item.id)), [1]);
  assert.equal(items.length, 5);
});

test('autocomplete suggestions include disabled items and are unique case-insensitively', () => {
  assert.deepEqual(uniqueSuggestions(items, 'name'), [
    'Apples',
    'Butter',
    'HP ink',
    'Milk',
    'Screwdriver',
  ]);
  assert.deepEqual(uniqueSuggestions(items, 'category'), ['Dairy', 'Hardware']);
});

test('view settings validate, serialize and become a session cookie', () => {
  const parsed = parseSettings('{"sort":"name_asc","category":"HARDWARE","hideDisabled":true}');
  assert.deepEqual(parsed, {
    sort: 'name_asc',
    category: 'HARDWARE',
    hideDisabled: true,
  });

  assert.deepEqual(parseSettings('not json'), {
    sort: 'created_asc',
    category: '',
    hideDisabled: false,
  });
  assert.deepEqual(parseSettings({ sort: 'bogus', hideDisabled: 'yes' }), {
    sort: 'created_asc',
    category: '',
    hideDisabled: false,
  });

  assert.equal(serializeSettings(parsed), '{"sort":"name_asc","category":"HARDWARE","hideDisabled":true}');

  const httpCookie = cookieString(parsed, false);
  assert.match(httpCookie, /^2buy_view=/);
  assert.match(httpCookie, /Path=\/; SameSite=Lax$/);
  assert.doesNotMatch(httpCookie, /Secure/);
  assert.doesNotMatch(httpCookie, /Expires|Max-Age/);

  const httpsCookie = cookieString(parsed, true);
  assert.match(httpsCookie, /; Secure$/);

  const stored = encodeURIComponent(serializeSettings(parsed));
  assert.deepEqual(
    parseSettings(readCookieValue(`other=1; 2buy_view=${stored}; theme=dark`, '2buy_view')),
    parsed
  );
  assert.equal(readCookieValue('other=1', '2buy_view'), '');
});

test('API path and username are derived from the bookmark URL', () => {
  assert.equal(apiBaseFromPath('/u/alice/secrettoken1234567890/'), '/api/alice/secrettoken1234567890');
  assert.equal(apiBaseFromPath('/u/alice/secrettoken1234567890'), '/api/alice/secrettoken1234567890');
  assert.equal(usernameFromPath('/u/alice/secrettoken1234567890/'), 'alice');
  assert.equal(usernameFromPath('/api/alice/secrettoken1234567890/items'), '');
});

test('reverse-proxy prefix is preserved in API paths and usernames', () => {
  assert.equal(
    apiBaseFromPath('/2buy/u/alice/secrettoken1234567890/'),
    '/2buy/api/alice/secrettoken1234567890'
  );
  assert.equal(
    apiBaseFromPath('/2buy/u/alice/secrettoken1234567890'),
    '/2buy/api/alice/secrettoken1234567890'
  );
  assert.equal(usernameFromPath('/2buy/u/alice/secrettoken1234567890/'), 'alice');

  assert.equal(
    apiBaseFromPath('/shop/2buy/u/bob/secrettoken1234567890/style.css'),
    '/shop/2buy/api/bob/secrettoken1234567890'
  );
  assert.equal(usernameFromPath('/shop/2buy/u/bob/secrettoken1234567890/style.css'), 'bob');

  assert.equal(usernameFromPath('/2buy/api/alice/secrettoken1234567890/items'), '');
});
