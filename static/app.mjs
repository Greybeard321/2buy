import {
  apiBaseFromPath,
  cookieString,
  parseSettings,
  readCookieValue,
  uniqueSuggestions,
  usernameFromPath,
  visibleGroups,
} from './logic.mjs';

const apiBase = apiBaseFromPath(window.location.pathname);
const username = usernameFromPath(window.location.pathname);

const els = {
  username: document.getElementById('username'),
  groups: document.getElementById('groups'),
  empty: document.getElementById('empty'),
  message: document.getElementById('message'),
  refresh: document.getElementById('refresh'),
  sort: document.getElementById('sort'),
  category: document.getElementById('category-filter'),
  hideDisabled: document.getElementById('hide-disabled'),
  add: document.getElementById('add-item'),
  dialog: document.getElementById('item-dialog'),
  form: document.getElementById('item-form'),
  dialogTitle: document.getElementById('dialog-title'),
  itemId: document.getElementById('item-id'),
  name: document.getElementById('field-name'),
  quantity: document.getElementById('field-quantity'),
  categoryInput: document.getElementById('field-category'),
  comment: document.getElementById('field-comment'),
  reuseNotice: document.getElementById('reuse-notice'),
  formError: document.getElementById('form-error'),
  cancel: document.getElementById('cancel'),
  nameList: document.getElementById('name-suggestions'),
  categoryList: document.getElementById('category-suggestions'),
};

let items = [];
let settings = parseSettings(readCookieValue(document.cookie, '2buy_view'));
let editingItem = null;
let disabled = false;

function showMessage(text, isError = false) {
  els.message.textContent = text;
  els.message.className = isError ? 'error' : 'notice';
  els.message.hidden = false;
}

function clearMessage() {
  els.message.hidden = true;
  els.message.textContent = '';
}

function stopOnForbidden() {
  disabled = true;
  els.add.disabled = true;
  els.refresh.disabled = true;
  els.sort.disabled = true;
  els.category.disabled = true;
  els.hideDisabled.disabled = true;
  showMessage('This link is no longer valid. Ask the admin for a new URL.', true);
}

function saveSettings() {
  const secure = window.location.protocol === 'https:';
  document.cookie = cookieString(settings, secure);
}

async function api(path, options = {}) {
  const response = await fetch(`${apiBase}${path}`, {
    ...options,
    headers: { 'Content-Type': 'application/json', ...(options.headers || {}) },
  });

  if (response.status === 403) {
    stopOnForbidden();
    throw new Error('forbidden');
  }

  const text = await response.text();
  let body = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      body = null;
    }
  }

  if (!response.ok) {
    const message = body && body.error ? body.error : `Request failed (${response.status})`;
    throw new Error(message);
  }
  return body;
}

function fillSuggestions() {
  els.nameList.replaceChildren();
  for (const value of uniqueSuggestions(items, 'name')) {
    const option = document.createElement('option');
    option.value = value;
    els.nameList.append(option);
  }

  els.categoryList.replaceChildren();
  for (const value of uniqueSuggestions(items, 'category')) {
    const option = document.createElement('option');
    option.value = value;
    els.categoryList.append(option);
  }
}

function rebuildCategoryFilter(groups) {
  const previous = settings.category;
  els.category.replaceChildren();

  const all = document.createElement('option');
  all.value = '';
  all.textContent = 'All categories';
  els.category.append(all);

  for (const group of groups) {
    const option = document.createElement('option');
    option.value = group.key;
    option.textContent = group.label;
    els.category.append(option);
  }

  const stillExists = groups.some((group) => group.key === previous);
  settings.category = stillExists ? previous : '';
  els.category.value = settings.category;
}

function button(label, action, className = 'secondary') {
  const el = document.createElement('button');
  el.type = 'button';
  el.textContent = label;
  el.className = className;
  el.dataset.action = action;
  return el;
}

function itemElement(item) {
  const li = document.createElement('li');
  li.className = item.disabled ? 'item is-disabled' : 'item';
  li.dataset.id = String(item.id);

  const text = document.createElement('div');
  text.className = 'item-text';

  const name = document.createElement('span');
  name.className = 'item-name';
  name.textContent = item.name;

  const meta = document.createElement('span');
  meta.className = 'item-meta';
  meta.textContent = `${item.quantity} · ${item.category}${item.comment ? ` · ${item.comment}` : ''}`;

  text.append(name, meta);

  const actions = document.createElement('div');
  actions.className = 'item-actions';
  actions.append(
    button('Edit', 'edit'),
    button(item.disabled ? 'Enable' : 'Disable', 'toggle'),
    button('Remove', 'remove', 'danger')
  );

  li.append(text, actions);
  return li;
}

function render() {
  const groups = visibleGroups(items, settings);
  rebuildCategoryFilter(groups);

  els.sort.value = settings.sort;
  els.hideDisabled.checked = settings.hideDisabled;

  els.groups.replaceChildren();
  for (const group of groups) {
    const section = document.createElement('section');
    section.className = 'group';

    const heading = document.createElement('h2');
    heading.textContent = group.label;

    const list = document.createElement('ul');
    list.className = 'items';
    for (const item of group.items) {
      list.append(itemElement(item));
    }

    section.append(heading, list);
    els.groups.append(section);
  }

  els.empty.hidden = groups.length > 0;
}

async function load() {
  if (disabled) return;
  try {
    items = await api('/items');
    fillSuggestions();
    render();
  } catch (err) {
    if (err.message !== 'forbidden') {
      showMessage(err.message, true);
    }
  }
}

function openDialog(item = null) {
  editingItem = item;
  els.formError.hidden = true;
  els.reuseNotice.hidden = true;

  els.dialogTitle.textContent = item ? 'Edit item' : 'Add item';
  els.itemId.value = item ? String(item.id) : '';
  els.name.value = item ? item.name : '';
  els.quantity.value = item ? item.quantity : '1';
  els.categoryInput.value = item ? item.category : '';
  els.comment.value = item ? item.comment : '';

  els.dialog.showModal();
  els.name.focus();
}

function updateReuseNotice() {
  const name = els.name.value.trim().toLocaleLowerCase();
  const match = items.find(
    (item) => item.name.trim().toLocaleLowerCase() === name && item.id !== editingItem?.id
  );
  els.reuseNotice.hidden = !match;
  if (match) {
    els.reuseNotice.textContent = `“${match.name}” already exists. Saving will update and re-enable it.`;
  }
}

async function saveItem(event) {
  event.preventDefault();
  els.formError.hidden = true;

  const payload = {
    name: els.name.value.trim(),
    quantity: els.quantity.value.trim() || '1',
    category: els.categoryInput.value.trim(),
    comment: els.comment.value.trim(),
    disabled: editingItem ? editingItem.disabled : false,
  };

  try {
    if (editingItem) {
      await api(`/items/${editingItem.id}`, {
        method: 'PUT',
        body: JSON.stringify(payload),
      });
      showMessage('Item updated.');
    } else {
      const result = await api('/items', {
        method: 'POST',
        body: JSON.stringify(payload),
      });
      showMessage(
        result.result === 'updated'
          ? 'Existing item updated and re-enabled.'
          : 'Item added.'
      );
    }
    els.dialog.close();
    await load();
  } catch (err) {
    if (err.message !== 'forbidden') {
      els.formError.textContent = err.message;
      els.formError.hidden = false;
    }
  }
}

els.username.textContent = username;

els.refresh.addEventListener('click', () => {
  clearMessage();
  load();
});

els.add.addEventListener('click', () => openDialog());
els.cancel.addEventListener('click', () => els.dialog.close());
els.form.addEventListener('submit', saveItem);
els.name.addEventListener('input', updateReuseNotice);

els.sort.addEventListener('change', () => {
  settings.sort = els.sort.value;
  saveSettings();
  render();
});

els.category.addEventListener('change', () => {
  settings.category = els.category.value;
  saveSettings();
  render();
});

els.hideDisabled.addEventListener('change', () => {
  settings.hideDisabled = els.hideDisabled.checked;
  saveSettings();
  render();
});

els.groups.addEventListener('click', async (event) => {
  const target = event.target.closest('button[data-action]');
  if (!target) return;

  const row = target.closest('li[data-id]');
  const id = Number(row.dataset.id);
  const item = items.find((entry) => entry.id === id);
  if (!item) return;

  const action = target.dataset.action;
  if (action === 'edit') {
    openDialog(item);
    return;
  }

  if (action === 'toggle') {
    try {
      await api(`/items/${id}`, {
        method: 'PATCH',
        body: JSON.stringify({ disabled: !item.disabled }),
      });
      await load();
    } catch (err) {
      if (err.message !== 'forbidden') showMessage(err.message, true);
    }
    return;
  }

  if (action === 'remove') {
    if (!window.confirm(`Remove “${item.name}” from the list?`)) return;
    try {
      await api(`/items/${id}`, { method: 'DELETE' });
      await load();
    } catch (err) {
      if (err.message !== 'forbidden') showMessage(err.message, true);
    }
  }
});

load();
