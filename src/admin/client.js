// SquirrelDB Admin - Client-side JavaScript

// =============================================================================
// Authentication
// =============================================================================

const Auth = {
  TOKEN_KEY: 'sqrl_admin_token',

  getToken() {
    return localStorage.getItem(this.TOKEN_KEY);
  },

  setToken(token) {
    if (token) {
      localStorage.setItem(this.TOKEN_KEY, token);
    } else {
      localStorage.removeItem(this.TOKEN_KEY);
    }
  },

  clearToken() {
    localStorage.removeItem(this.TOKEN_KEY);
  },

  isRequired() {
    return window.SQRL_AUTH_ENABLED === true;
  },

  getHeaders() {
    const headers = { 'Content-Type': 'application/json' };
    const token = this.getToken();
    if (token) {
      headers['Authorization'] = `Bearer ${token}`;
    }
    return headers;
  }
};

// =============================================================================
// API Client
// =============================================================================

const API = {
  async request(method, path, data = null) {
    const options = {
      method,
      headers: Auth.getHeaders()
    };
    if (data !== null) {
      options.body = JSON.stringify(data);
    }

    const res = await fetch(`/api${path}`, options);

    // Handle 401 - redirect to login page
    if (res.status === 401) {
      if (Auth.isRequired()) {
        Auth.clearToken();
        window.location.href = '/login';
        throw new Error('Authentication required');
      }
    }

    if (!res.ok) {
      const text = await res.text();
      throw new Error(text);
    }
    return res.json();
  },

  async get(path) {
    return this.request('GET', path);
  },
  async post(path, data) {
    return this.request('POST', path, data);
  },
  async put(path, data) {
    return this.request('PUT', path, data);
  },
  async delete(path) {
    return this.request('DELETE', path);
  }
};

// =============================================================================
// State
// =============================================================================

let currentTable = null;
let currentDoc = null;
let collections = [];

// Live streaming state
let liveSocket = null;
let liveStats = { total: 0, insert: 0, update: 0, delete: 0 };
let liveSubscribedTable = null;

// =============================================================================
// Initialization
// =============================================================================

document.addEventListener('DOMContentLoaded', () => {
  // Initialize theme
  initTheme();

  // Check if first-time setup is needed
  if (window.SQRL_SETUP_NEEDED === true) {
    window.location.href = '/setup';
    return;
  }

  // Check auth requirement
  if (Auth.isRequired() && !Auth.getToken()) {
    // Redirect to login page instead of showing modal
    window.location.href = '/login';
    return;
  }

  // Load initial data
  loadDashboard();
  loadCollections();

  // Auto-refresh dashboard every 5s when visible
  setInterval(() => {
    if (document.getElementById('dashboard')?.classList.contains('active')) {
      loadDashboard();
    }
  }, 5000);

  // Keyboard shortcuts
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      hideModal();
      hideTableMenu();
    }
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      const saveBtn = document.querySelector('#modal-footer .btn-primary');
      if (saveBtn && !saveBtn.disabled) {
        saveBtn.click();
      }
    }
  });

  // Close dropdown when clicking outside
  document.addEventListener('click', (e) => {
    if (!e.target.closest('.dropdown')) {
      hideTableMenu();
    }
  });
});

// =============================================================================
// Theme Management
// =============================================================================

function initTheme() {
  const saved = localStorage.getItem('squirreldb-theme') || 'system';
  applyTheme(saved);
  updateThemeButtons(saved);
}

window.setTheme = (theme) => {
  localStorage.setItem('squirreldb-theme', theme);
  applyTheme(theme);
  updateThemeButtons(theme);
};

function applyTheme(theme) {
  document.documentElement.setAttribute('data-theme', theme);
}

function updateThemeButtons(active) {
  document.querySelectorAll('.theme-btn').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.theme === active);
  });
}

// =============================================================================
// Dashboard
// =============================================================================

async function loadDashboard() {
  try {
    const [status, cols] = await Promise.all([
      API.get('/status'),
      API.get('/collections')
    ]);

    collections = cols;

    // Update stats
    document.getElementById('stat-tables').textContent = cols.length;
    document.getElementById('stat-documents').textContent =
      cols.reduce((sum, c) => sum + c.count, 0);
    document.getElementById('stat-backend').textContent = status.backend;
    document.getElementById('stat-uptime').textContent = formatUptime(status.uptime_secs);

    // Update server status
    document.getElementById('server-status-text').textContent = 'Connected';
    document.getElementById('server-status-dot').classList.remove('disconnected');

    // Update footer version
    document.getElementById('sidebar-footer-info').textContent = `v${status.version || '0.1.0'}`;

    // Update tables list
    const tablesList = document.getElementById('tables-list');
    if (cols.length === 0) {
      tablesList.innerHTML = `
        <tr>
          <td colspan="3" class="text-secondary" style="text-align: center; padding: 32px;">
            No tables yet. Click "New Table" to create one.
          </td>
        </tr>
      `;
    } else {
      tablesList.innerHTML = cols.map(c => `
        <tr>
          <td><strong>${escapeHtml(c.name)}</strong></td>
          <td>${c.count}</td>
          <td class="actions">
            <button class="btn btn-secondary btn-sm" onclick="viewTable('${escapeHtml(c.name)}')">View</button>
            <button class="btn btn-danger btn-sm" onclick="confirmDropTable('${escapeHtml(c.name)}')">Drop</button>
          </td>
        </tr>
      `).join('');
    }

    // Update sidebar tables and table selector
    updateSidebarTables(cols);
    updateTableSelector(cols);

  } catch (e) {
    console.error('Failed to load dashboard:', e);
    document.getElementById('server-status-text').textContent = 'Disconnected';
    document.getElementById('server-status-dot').classList.add('disconnected');
    showToast('Failed to connect to server', 'error');
  }
}

async function loadCollections() {
  try {
    collections = await API.get('/collections');
    updateSidebarTables(collections);
    updateTableSelector(collections);
    updateLiveTableSelector(collections);
  } catch (e) {
    console.error('Failed to load collections:', e);
  }
}

function formatUptime(secs) {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
  return `${Math.floor(secs / 86400)}d ${Math.floor((secs % 86400) / 3600)}h`;
}

function updateSidebarTables(cols) {
  const sidebarTables = document.getElementById('sidebar-tables');
  if (!sidebarTables) return;

  if (cols.length === 0) {
    sidebarTables.innerHTML = `
      <li class="text-muted" style="padding: 8px 12px; font-size: 12px;">
        No tables yet
      </li>
    `;
    return;
  }

  sidebarTables.innerHTML = cols.map(c => `
    <li>
      <div class="table-quick-item ${currentTable === c.name ? 'active' : ''}"
           onclick="viewTable('${escapeHtml(c.name)}')">
        <span>${escapeHtml(c.name)}</span>
        <span class="table-quick-count">${c.count}</span>
      </div>
    </li>
  `).join('');
}

function updateTableSelector(cols) {
  const selector = document.getElementById('table-selector');
  if (!selector) return;

  const currentValue = selector.value;
  selector.innerHTML = `
    <option value="">Select a table...</option>
    ${cols.map(c => `
      <option value="${escapeHtml(c.name)}" ${c.name === currentValue ? 'selected' : ''}>
        ${escapeHtml(c.name)} (${c.count})
      </option>
    `).join('')}
  `;
}

// =============================================================================
// Navigation
// =============================================================================

window.showPage = (page) => {
  document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
  document.getElementById(page)?.classList.add('active');

  document.querySelectorAll('.nav-link').forEach(link => {
    link.classList.toggle('active', link.dataset.page === page);
  });

  if (page === 'dashboard') {
    loadDashboard();
  } else if (page === 'settings') {
    loadSettings();
  }
};

// =============================================================================
// Table Management
// =============================================================================

window.createTable = () => {
  showModal('create-table', {
    title: 'Create New Table'
  });
};

window.saveNewTable = async () => {
  const nameInput = document.getElementById('table-name-input');
  const name = nameInput.value.trim();

  if (!name) {
    showToast('Please enter a table name', 'warning');
    return;
  }

  // Validate table name (alphanumeric and underscores only)
  if (!/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(name)) {
    showToast('Table name must start with a letter and contain only letters, numbers, and underscores', 'warning');
    return;
  }

  try {
    // Create table by inserting an empty document
    await API.post(`/collections/${encodeURIComponent(name)}/documents`, {});
    showToast(`Table "${name}" created`, 'success');
    hideModal();
    await loadDashboard();
    viewTable(name);
  } catch (e) {
    showToast(`Failed to create table: ${e.message}`, 'error');
  }
};

window.viewTable = async (name) => {
  currentTable = name;
  showPage('tables');

  const selector = document.getElementById('table-selector');
  if (selector) selector.value = name;

  // Enable all table action buttons
  document.getElementById('insert-btn')?.removeAttribute('disabled');
  document.getElementById('export-btn')?.removeAttribute('disabled');
  document.getElementById('refresh-btn')?.removeAttribute('disabled');
  document.getElementById('table-menu-btn')?.removeAttribute('disabled');

  await loadTableDocs(name);
  updateSidebarTables(collections);
};

window.onTableSelect = async (name) => {
  if (!name) {
    currentTable = null;
    document.getElementById('insert-btn')?.setAttribute('disabled', '');
    document.getElementById('export-btn')?.setAttribute('disabled', '');
    document.getElementById('refresh-btn')?.setAttribute('disabled', '');
    document.getElementById('table-menu-btn')?.setAttribute('disabled', '');
    document.getElementById('documents-grid').innerHTML = `
      <div class="empty-state">
        <p>Select a table to view documents</p>
      </div>
    `;
    return;
  }

  currentTable = name;
  document.getElementById('insert-btn')?.removeAttribute('disabled');
  document.getElementById('export-btn')?.removeAttribute('disabled');
  document.getElementById('refresh-btn')?.removeAttribute('disabled');
  document.getElementById('table-menu-btn')?.removeAttribute('disabled');
  await loadTableDocs(name);
  updateSidebarTables(collections);
};

window.refreshTable = async () => {
  if (currentTable) {
    await loadTableDocs(currentTable);
    showToast('Table refreshed', 'success');
  }
};

async function loadTableDocs(name) {
  const grid = document.getElementById('documents-grid');

  try {
    const docs = await API.get(`/collections/${encodeURIComponent(name)}?limit=100`);

    if (docs.length === 0) {
      grid.innerHTML = `
        <div class="empty-state">
          <p>No documents in this table</p>
          <button class="btn btn-primary" onclick="insertDoc()">Insert Document</button>
        </div>
      `;
      return;
    }

    grid.innerHTML = docs.map(d => `
      <div class="document-card">
        <div class="document-header">
          <span class="document-id" title="${d.id}">${d.id.slice(0, 8)}...</span>
          <div class="document-actions">
            <button class="btn btn-ghost btn-sm" onclick="editDoc('${d.id}')" title="Edit">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                <path d="M11.013 1.427a1.75 1.75 0 012.474 0l1.086 1.086a1.75 1.75 0 010 2.474l-8.61 8.61c-.21.21-.47.364-.756.445l-3.251.93a.75.75 0 01-.927-.928l.929-3.25c.081-.286.235-.547.445-.758l8.61-8.61zm1.414 1.06a.25.25 0 00-.354 0L10.811 3.75l1.439 1.44 1.263-1.263a.25.25 0 000-.354l-1.086-1.086zM11.189 6.25L9.75 4.81l-6.286 6.287a.25.25 0 00-.064.108l-.558 1.953 1.953-.558a.249.249 0 00.108-.064l6.286-6.286z"/>
              </svg>
            </button>
            <button class="btn btn-ghost btn-sm" onclick="confirmDeleteDoc('${d.id}')" title="Delete">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                <path d="M6.5 1.75a.25.25 0 01.25-.25h2.5a.25.25 0 01.25.25V3h-3V1.75zm4.5 0V3h2.25a.75.75 0 010 1.5H2.75a.75.75 0 010-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75zM4.496 6.675a.75.75 0 10-1.492.15l.66 6.6A1.75 1.75 0 005.405 15h5.19c.9 0 1.652-.681 1.741-1.576l.66-6.6a.75.75 0 00-1.492-.149l-.66 6.6a.25.25 0 01-.249.225h-5.19a.25.25 0 01-.249-.225l-.66-6.6z"/>
              </svg>
            </button>
          </div>
        </div>
        <pre class="document-data">${escapeHtml(JSON.stringify(d.data, null, 2))}</pre>
      </div>
    `).join('');

  } catch (e) {
    console.error('Failed to load documents:', e);
    grid.innerHTML = `
      <div class="empty-state">
        <p class="text-danger">Failed to load documents</p>
        <p class="text-secondary">${escapeHtml(e.message)}</p>
      </div>
    `;
  }
}

// =============================================================================
// Table Dropdown Menu
// =============================================================================

window.toggleTableMenu = () => {
  const menu = document.getElementById('table-menu');
  menu.classList.toggle('show');
};

function hideTableMenu() {
  document.getElementById('table-menu')?.classList.remove('show');
}

// =============================================================================
// Export / Import
// =============================================================================

window.exportTable = async () => {
  if (!currentTable) return;

  try {
    const docs = await API.get(`/collections/${encodeURIComponent(currentTable)}?limit=10000`);
    const data = docs.map(d => d.data);
    const json = JSON.stringify(data, null, 2);

    // Create download
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${currentTable}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);

    showToast(`Exported ${docs.length} documents`, 'success');
  } catch (e) {
    showToast(`Export failed: ${e.message}`, 'error');
  }
};

window.importToTable = () => {
  hideTableMenu();
  if (!currentTable) return;

  showModal('import', {
    title: `Import to ${currentTable}`
  });
};

window.doImport = async () => {
  const textarea = document.getElementById('import-data');
  const errorEl = document.getElementById('import-error');
  const text = textarea.value.trim();

  if (!text) {
    errorEl.textContent = 'Please paste JSON data';
    errorEl.classList.remove('hidden');
    return;
  }

  let data;
  try {
    data = JSON.parse(text);
    if (!Array.isArray(data)) {
      data = [data]; // Wrap single object in array
    }
    errorEl.classList.add('hidden');
  } catch (e) {
    errorEl.textContent = `Invalid JSON: ${e.message}`;
    errorEl.classList.remove('hidden');
    return;
  }

  try {
    let imported = 0;
    for (const item of data) {
      await API.post(`/collections/${encodeURIComponent(currentTable)}/documents`, item);
      imported++;
    }

    showToast(`Imported ${imported} documents`, 'success');
    hideModal();
    await loadTableDocs(currentTable);
    await loadCollections();
  } catch (e) {
    showToast(`Import failed: ${e.message}`, 'error');
  }
};

window.clearTable = () => {
  hideTableMenu();
  if (!currentTable) return;

  showModal('confirm', {
    title: 'Clear Table Data',
    message: `Are you sure you want to delete <strong>all documents</strong> from <strong>${escapeHtml(currentTable)}</strong>?<br><br>This action cannot be undone.`,
    confirmText: 'Clear All',
    confirmClass: 'btn-danger',
    onConfirm: async () => {
      try {
        await API.delete(`/collections/${encodeURIComponent(currentTable)}`);
        showToast(`Cleared all data from "${currentTable}"`, 'success');
        hideModal();
        await loadTableDocs(currentTable);
        await loadCollections();
      } catch (e) {
        showToast(`Failed to clear table: ${e.message}`, 'error');
      }
    }
  });
};

// =============================================================================
// Document CRUD
// =============================================================================

window.insertDoc = () => {
  if (!currentTable) {
    showToast('Please select a table first', 'warning');
    return;
  }

  currentDoc = null;
  showModal('edit', {
    title: 'Insert Document',
    table: currentTable,
    data: {}
  });
};

window.editDoc = async (id) => {
  if (!currentTable) return;

  try {
    const doc = await API.get(`/collections/${encodeURIComponent(currentTable)}/documents/${id}`);
    currentDoc = { id, data: doc.data };
    showModal('edit', {
      title: 'Edit Document',
      table: currentTable,
      data: doc.data
    });
  } catch (e) {
    showToast('Failed to load document', 'error');
  }
};

window.saveDoc = async () => {
  const textarea = document.getElementById('json-editor');
  const errorEl = document.getElementById('json-error');

  let data;
  try {
    data = JSON.parse(textarea.value);
    textarea.classList.remove('invalid');
    errorEl.classList.add('hidden');
  } catch (e) {
    textarea.classList.add('invalid');
    errorEl.textContent = `Invalid JSON: ${e.message}`;
    errorEl.classList.remove('hidden');
    return;
  }

  try {
    if (currentDoc) {
      await API.put(
        `/collections/${encodeURIComponent(currentTable)}/documents/${currentDoc.id}`,
        data
      );
      showToast('Document updated', 'success');
    } else {
      await API.post(`/collections/${encodeURIComponent(currentTable)}/documents`, data);
      showToast('Document created', 'success');
    }

    hideModal();
    await loadTableDocs(currentTable);
    await loadCollections();

  } catch (e) {
    showToast(`Failed to save: ${e.message}`, 'error');
  }
};

window.confirmDeleteDoc = (id) => {
  showModal('confirm', {
    title: 'Delete Document',
    message: `Are you sure you want to delete this document?<br><br><code>${id}</code>`,
    confirmText: 'Delete',
    confirmClass: 'btn-danger',
    onConfirm: () => deleteDoc(id)
  });
};

async function deleteDoc(id) {
  try {
    await API.delete(`/collections/${encodeURIComponent(currentTable)}/documents/${id}`);
    showToast('Document deleted', 'success');
    hideModal();
    await loadTableDocs(currentTable);
    await loadCollections();
  } catch (e) {
    showToast(`Failed to delete: ${e.message}`, 'error');
  }
}

// =============================================================================
// Table Operations
// =============================================================================

window.confirmDropTable = (name) => {
  showModal('confirm', {
    title: 'Drop Table',
    message: `Are you sure you want to drop the table <strong>${escapeHtml(name)}</strong>?<br><br>This will delete all documents in this table. This action cannot be undone.`,
    confirmText: 'Drop Table',
    confirmClass: 'btn-danger',
    onConfirm: () => dropTable(name)
  });
};

async function dropTable(name) {
  try {
    await API.delete(`/collections/${encodeURIComponent(name)}`);
    showToast(`Table "${name}" dropped`, 'success');
    hideModal();

    if (currentTable === name) {
      currentTable = null;
      document.getElementById('table-selector').value = '';
      document.getElementById('insert-btn')?.setAttribute('disabled', '');
      document.getElementById('export-btn')?.setAttribute('disabled', '');
      document.getElementById('refresh-btn')?.setAttribute('disabled', '');
      document.getElementById('table-menu-btn')?.setAttribute('disabled', '');
      document.getElementById('documents-grid').innerHTML = `
        <div class="empty-state">
          <p>Select a table to view documents</p>
        </div>
      `;
    }

    await loadDashboard();

  } catch (e) {
    showToast(`Failed to drop table: ${e.message}`, 'error');
  }
}

// =============================================================================
// Query Explorer
// =============================================================================

window.runQuery = async () => {
  const input = document.getElementById('query-input');
  const results = document.getElementById('query-results');
  const timeEl = document.getElementById('query-time');

  const query = input.value.trim();
  if (!query) {
    results.textContent = 'Please enter a query.';
    return;
  }

  const start = performance.now();

  try {
    const data = await API.post('/query', { query });
    const elapsed = performance.now() - start;

    results.textContent = JSON.stringify(data, null, 2);
    timeEl.textContent = `${elapsed.toFixed(1)}ms · ${Array.isArray(data) ? data.length : 1} result(s)`;

    await loadCollections();

  } catch (e) {
    const elapsed = performance.now() - start;
    results.textContent = `Error: ${e.message}`;
    timeEl.textContent = `${elapsed.toFixed(1)}ms · Error`;
  }
};

// =============================================================================
// Modal System
// =============================================================================

let modalConfirmCallback = null;

function showModal(type, options) {
  const overlay = document.getElementById('modal-overlay');
  const title = document.getElementById('modal-title');
  const body = document.getElementById('modal-body');
  const footer = document.getElementById('modal-footer');

  title.textContent = options.title || 'Modal';

  if (type === 'edit') {
    body.innerHTML = `
      <label for="json-editor">Document Data (JSON)</label>
      <textarea id="json-editor" class="json-editor">${escapeHtml(JSON.stringify(options.data || {}, null, 2))}</textarea>
      <div id="json-error" class="json-error hidden"></div>
    `;
    footer.innerHTML = `
      <button class="btn btn-secondary" onclick="hideModal()">Cancel</button>
      <button class="btn btn-primary" onclick="saveDoc()">Save</button>
    `;

    setTimeout(() => {
      document.getElementById('json-editor')?.focus();
    }, 100);

  } else if (type === 'confirm') {
    body.innerHTML = `<p class="confirm-message">${options.message}</p>`;
    footer.innerHTML = `
      <button class="btn btn-secondary" onclick="hideModal()">Cancel</button>
      <button class="btn ${options.confirmClass || 'btn-primary'}" onclick="onModalConfirm()">${options.confirmText || 'Confirm'}</button>
    `;
    modalConfirmCallback = options.onConfirm;

  } else if (type === 'create-table') {
    body.innerHTML = `
      <div class="form-group">
        <label for="table-name-input">Table Name</label>
        <input type="text" id="table-name-input" class="input" placeholder="users" autocomplete="off">
        <p class="form-hint">Use letters, numbers, and underscores. Must start with a letter.</p>
      </div>
    `;
    footer.innerHTML = `
      <button class="btn btn-secondary" onclick="hideModal()">Cancel</button>
      <button class="btn btn-primary" onclick="saveNewTable()">Create Table</button>
    `;

    setTimeout(() => {
      const input = document.getElementById('table-name-input');
      input?.focus();
      input?.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') saveNewTable();
      });
    }, 100);

  } else if (type === 'import') {
    body.innerHTML = `
      <label for="import-data">Paste JSON Array</label>
      <textarea id="import-data" class="json-editor" placeholder='[{"name": "John"}, {"name": "Jane"}]'></textarea>
      <div id="import-error" class="json-error hidden"></div>
      <p class="form-hint">Paste a JSON array of objects, or a single object.</p>
    `;
    footer.innerHTML = `
      <button class="btn btn-secondary" onclick="hideModal()">Cancel</button>
      <button class="btn btn-primary" onclick="doImport()">Import</button>
    `;

    setTimeout(() => {
      document.getElementById('import-data')?.focus();
    }, 100);

  } else if (type === 'create-token') {
    body.innerHTML = `
      <div class="form-group">
        <label for="token-name-input">Token Name</label>
        <input type="text" id="token-name-input" class="input" placeholder="e.g., production-api" autocomplete="off">
        <p class="form-hint">A descriptive name to identify this token</p>
      </div>
    `;
    footer.innerHTML = `
      <button class="btn btn-secondary" onclick="hideModal()">Cancel</button>
      <button class="btn btn-primary" onclick="createToken()">Generate</button>
    `;

    setTimeout(() => {
      const input = document.getElementById('token-name-input');
      input?.focus();
      input?.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') createToken();
      });
    }, 100);

  } else if (type === 'token-created') {
    body.innerHTML = `
      <div class="token-display">
        <p class="token-warning">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8.982 1.566a1.13 1.13 0 00-1.96 0L.165 13.233c-.457.778.091 1.767.98 1.767h13.713c.889 0 1.438-.99.98-1.767L8.982 1.566zM8 5c.535 0 .954.462.9.995l-.35 3.507a.552.552 0 01-1.1 0L7.1 5.995A.905.905 0 018 5zm.002 6a1 1 0 110 2 1 1 0 010-2z"/>
          </svg>
          <strong>Save this token now!</strong> It will only be shown once.
        </p>
        <div class="token-value-container">
          <code class="token-value">${escapeHtml(options.token)}</code>
          <button class="btn btn-secondary btn-sm" onclick="copyToken('${escapeHtml(options.token)}')">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M0 6.75C0 5.784.784 5 1.75 5h1.5a.75.75 0 010 1.5h-1.5a.25.25 0 00-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 00.25-.25v-1.5a.75.75 0 011.5 0v1.5A1.75 1.75 0 019.25 16h-7.5A1.75 1.75 0 010 14.25v-7.5z"/>
              <path d="M5 1.75C5 .784 5.784 0 6.75 0h7.5C15.216 0 16 .784 16 1.75v7.5A1.75 1.75 0 0114.25 11h-7.5A1.75 1.75 0 015 9.25v-7.5zm1.75-.25a.25.25 0 00-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 00.25-.25v-7.5a.25.25 0 00-.25-.25h-7.5z"/>
            </svg>
            Copy
          </button>
        </div>
        <p class="form-hint">Use this token in the <code>Authorization</code> header: <code>Bearer ${escapeHtml(options.token)}</code></p>
      </div>
    `;
    footer.innerHTML = `
      <button class="btn btn-primary" onclick="hideModal()">Done</button>
    `;

  } else if (type === 'login') {
    body.innerHTML = `
      <div class="login-form">
        <p class="login-description">Authentication is required to access the admin UI.</p>
        <div class="form-group">
          <label for="login-token-input">API Token or Admin Token</label>
          <input type="password" id="login-token-input" class="input" placeholder="sqrl_..." autocomplete="off">
          <p class="form-hint">Enter your API token or admin token</p>
        </div>
        <div id="login-error" class="json-error hidden"></div>
      </div>
    `;
    footer.innerHTML = `
      <button class="btn btn-primary" onclick="submitLogin()">Login</button>
    `;

    setTimeout(() => {
      const input = document.getElementById('login-token-input');
      input?.focus();
      input?.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') submitLogin();
      });
    }, 100);
  }

  overlay.classList.add('active');
}

window.hideModal = () => {
  const overlay = document.getElementById('modal-overlay');
  // Don't allow closing login modal without authentication
  if (Auth.isRequired() && !Auth.getToken()) {
    return;
  }
  overlay.classList.remove('active');
  modalConfirmCallback = null;
  currentDoc = null;
};

window.onModalConfirm = () => {
  if (modalConfirmCallback) {
    modalConfirmCallback();
  }
};

// Login modal functions
function showLoginModal() {
  showModal('login', { title: 'Authentication Required' });
}

window.submitLogin = async () => {
  const input = document.getElementById('login-token-input');
  const errorDiv = document.getElementById('login-error');
  const token = input?.value.trim();

  if (!token) {
    errorDiv.textContent = 'Please enter a token';
    errorDiv.classList.remove('hidden');
    return;
  }

  // Test the token by making a request
  Auth.setToken(token);
  try {
    await API.get('/settings');
    // Success - close modal and load data
    const overlay = document.getElementById('modal-overlay');
    overlay.classList.remove('active');
    showToast('Authenticated successfully', 'success');

    // Load initial data
    loadDashboard();
    loadCollections();
  } catch (e) {
    Auth.clearToken();
    errorDiv.textContent = 'Invalid token. Please try again.';
    errorDiv.classList.remove('hidden');
  }
};

window.logout = () => {
  Auth.clearToken();
  showToast('Logged out', 'success');
  if (Auth.isRequired()) {
    window.location.href = '/login';
  }
};

window.onModalOverlayClick = (e) => {
  if (e.target.id === 'modal-overlay') {
    hideModal();
  }
};

// =============================================================================
// Toast Notifications
// =============================================================================

function showToast(message, type = 'info') {
  const container = document.getElementById('toast-container');
  if (!container) return;

  const toast = document.createElement('div');
  toast.className = `toast ${type}`;
  toast.innerHTML = `<span class="toast-message">${escapeHtml(message)}</span>`;

  container.appendChild(toast);

  requestAnimationFrame(() => {
    toast.classList.add('show');
  });

  setTimeout(() => {
    toast.classList.remove('show');
    setTimeout(() => toast.remove(), 300);
  }, 3000);
}

// =============================================================================
// Live Streaming
// =============================================================================

function updateLiveTableSelector(cols) {
  const selector = document.getElementById('live-table-selector');
  if (!selector) return;

  const currentValue = selector.value;
  selector.innerHTML = `
    <option value="">All tables</option>
    ${cols.map(c => `
      <option value="${escapeHtml(c.name)}" ${c.name === currentValue ? 'selected' : ''}>
        ${escapeHtml(c.name)}
      </option>
    `).join('')}
  `;
}

window.onLiveTableChange = () => {
  // If connected, we'd need to re-subscribe
  // For now, just store the selection
  const selector = document.getElementById('live-table-selector');
  if (liveSocket && liveSocket.readyState === WebSocket.OPEN) {
    // Reconnect with new table filter
    disconnectLive();
    setTimeout(() => connectLive(), 100);
  }
};

window.connectLive = () => {
  if (liveSocket && liveSocket.readyState === WebSocket.OPEN) {
    return;
  }

  const tableSelector = document.getElementById('live-table-selector');
  const selectedTable = tableSelector?.value || null;
  liveSubscribedTable = selectedTable;

  // Determine WebSocket URL (same host, different protocol)
  // Note: Data WebSocket does not require auth (auth is only for admin UI)
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const wsUrl = `${protocol}//${window.location.host}/ws`;

  try {
    liveSocket = new WebSocket(wsUrl);

    liveSocket.onopen = () => {
      updateLiveConnectionStatus(true);
      showToast('Connected to live feed', 'success');

      // Subscribe to changes - build query string
      const table = selectedTable || collections[0]?.name || 'default';
      const filter = document.getElementById('live-filter')?.value || '';

      // Build query: db.table("name").changes() or with filter
      let query = `db.table("${table}")`;
      if (filter) {
        query += `.filter(${filter})`;
      }
      query += '.changes()';

      const subscribeMsg = {
        type: 'subscribe',
        id: 'live-' + Date.now(),
        query: query
      };
      liveSocket.send(JSON.stringify(subscribeMsg));
    };

    liveSocket.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data);
        handleLiveMessage(msg);
      } catch (e) {
        console.error('Failed to parse WebSocket message:', e);
      }
    };

    liveSocket.onclose = () => {
      updateLiveConnectionStatus(false);
      liveSocket = null;
    };

    liveSocket.onerror = (error) => {
      console.error('WebSocket error:', error);
      showToast('Connection error', 'error');
      updateLiveConnectionStatus(false);
    };

  } catch (e) {
    console.error('Failed to connect:', e);
    showToast('Failed to connect: ' + e.message, 'error');
  }
};

window.disconnectLive = () => {
  if (liveSocket) {
    // Send unsubscribe before closing
    if (liveSocket.readyState === WebSocket.OPEN) {
      liveSocket.send(JSON.stringify({
        type: 'unsubscribe',
        id: 'live-' + Date.now()
      }));
    }
    liveSocket.close();
    liveSocket = null;
  }
  updateLiveConnectionStatus(false);
  showToast('Disconnected from live feed', 'success');
};

window.clearLiveFeed = () => {
  const feed = document.getElementById('live-feed');
  if (feed) {
    feed.innerHTML = `
      <div class="empty-state">
        <svg width="48" height="48" viewBox="0 0 16 16" fill="currentColor" style="opacity: 0.2">
          <path d="M3.05 3.05a7 7 0 000 9.9.5.5 0 01-.707.707 8 8 0 010-11.314.5.5 0 01.707.707zm9.9-.707a.5.5 0 01.707 0 8 8 0 010 11.314.5.5 0 01-.707-.707 7 7 0 000-9.9.5.5 0 010-.707z"/>
          <path d="M4.464 4.465a.5.5 0 01-.707 0 5 5 0 000 7.072.5.5 0 01-.707.707 6 6 0 010-8.486.5.5 0 01.707.707zm7.072-.707a6 6 0 010 8.486.5.5 0 01-.707-.707 5 5 0 000-7.072.5.5 0 11.707-.707z"/>
          <path d="M8 10a2 2 0 100-4 2 2 0 000 4z"/>
        </svg>
        <p>${liveSocket ? 'Waiting for changes...' : 'Click Connect to start watching for changes'}</p>
        <p class="text-muted">Changes will appear here in real-time</p>
      </div>
    `;
  }

  // Reset stats
  liveStats = { total: 0, insert: 0, update: 0, delete: 0 };
  updateLiveStats();
};

function updateLiveConnectionStatus(connected) {
  const dot = document.getElementById('ws-status-dot');
  const text = document.getElementById('ws-status-text');
  const connectBtn = document.getElementById('live-connect-btn');
  const disconnectBtn = document.getElementById('live-disconnect-btn');

  if (dot) {
    dot.classList.toggle('disconnected', !connected);
  }
  if (text) {
    text.textContent = connected ? 'Connected' : 'Disconnected';
  }
  if (connectBtn) {
    connectBtn.disabled = connected;
  }
  if (disconnectBtn) {
    disconnectBtn.disabled = !connected;
  }
}

function handleLiveMessage(msg) {
  // Handle different message types from SquirrelDB (lowercase type tags)
  if (msg.type === 'change' && msg.change) {
    addLiveEvent(msg.change);
  } else if (msg.type === 'subscribed') {
    showToast('Subscribed to changes', 'success');
    // Clear and show waiting message
    const feed = document.getElementById('live-feed');
    if (feed) {
      feed.innerHTML = `
        <div class="empty-state">
          <svg width="48" height="48" viewBox="0 0 16 16" fill="currentColor" style="opacity: 0.2">
            <path d="M3.05 3.05a7 7 0 000 9.9.5.5 0 01-.707.707 8 8 0 010-11.314.5.5 0 01.707.707zm9.9-.707a.5.5 0 01.707 0 8 8 0 010 11.314.5.5 0 01-.707-.707 7 7 0 000-9.9.5.5 0 010-.707z"/>
            <path d="M4.464 4.465a.5.5 0 01-.707 0 5 5 0 000 7.072.5.5 0 01-.707.707 6 6 0 010-8.486.5.5 0 01.707.707zm7.072-.707a6 6 0 010 8.486.5.5 0 01-.707-.707 5 5 0 000-7.072.5.5 0 11.707-.707z"/>
            <path d="M8 10a2 2 0 100-4 2 2 0 000 4z"/>
          </svg>
          <p>Listening for changes...</p>
          <p class="text-muted">Make changes to the database to see them appear here</p>
        </div>
      `;
    }
  } else if (msg.type === 'error') {
    showToast(`Error: ${msg.error}`, 'error');
  } else if (msg.type === 'unsubscribed') {
    showToast('Unsubscribed from changes', 'success');
  }
}

function addLiveEvent(change) {
  const feed = document.getElementById('live-feed');
  if (!feed) return;

  // Clear empty state if present
  const emptyState = feed.querySelector('.empty-state');
  if (emptyState) {
    emptyState.remove();
  }

  // Determine event type from ChangeEvent (type field: initial, insert, update, delete)
  const opType = change.type || 'unknown';
  let eventClass = '';
  if (opType === 'insert' || opType === 'initial') {
    eventClass = 'insert';
    liveStats.insert++;
  } else if (opType === 'update') {
    eventClass = 'update';
    liveStats.update++;
  } else if (opType === 'delete') {
    eventClass = 'delete';
    liveStats.delete++;
  }
  liveStats.total++;

  // Format the data - ChangeEvent has 'new' (Document) or 'old' (Document/Value)
  const doc = change.new || change.document || change.old;
  const data = doc?.data || doc || change;
  const dataStr = JSON.stringify(data, null, 2);
  const table = doc?.collection || liveSubscribedTable || 'unknown';
  const time = new Date().toLocaleTimeString();

  // Create event element
  const eventEl = document.createElement('div');
  eventEl.className = `live-event ${eventClass}`;
  eventEl.innerHTML = `
    <div class="live-event-type">${opType}</div>
    <div class="live-event-content">
      <div class="live-event-table">Table: <strong>${escapeHtml(table)}</strong></div>
      <pre class="live-event-data">${escapeHtml(dataStr)}</pre>
    </div>
    <div class="live-event-time">${time}</div>
  `;

  // Insert at the top
  feed.insertBefore(eventEl, feed.firstChild);

  // Limit to 100 events
  while (feed.children.length > 100) {
    feed.removeChild(feed.lastChild);
  }

  // Update stats display
  updateLiveStats();
}

function updateLiveStats() {
  document.getElementById('live-stat-total').textContent = liveStats.total;
  document.getElementById('live-stat-insert').textContent = liveStats.insert;
  document.getElementById('live-stat-update').textContent = liveStats.update;
  document.getElementById('live-stat-delete').textContent = liveStats.delete;
}

// =============================================================================
// Utilities
// =============================================================================

function escapeHtml(str) {
  if (typeof str !== 'string') return str;
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

// =============================================================================
// Settings Page
// =============================================================================

let settingsLoaded = false;

async function loadSettings() {
  try {
    const settings = await API.get('/settings');

    // Update protocol toggles
    document.getElementById('setting-rest').checked = settings.protocols.rest;
    document.getElementById('setting-websocket').checked = settings.protocols.websocket;
    document.getElementById('setting-sse').checked = settings.protocols.sse;

    // Update auth toggle
    document.getElementById('setting-auth-enabled').checked = settings.auth.enabled;

    // Show/hide auth warning
    const authWarning = document.getElementById('auth-warning');
    if (authWarning) {
      authWarning.style.display = settings.auth.enabled ? 'none' : 'flex';
    }

    // Load tokens
    await loadTokens();

    settingsLoaded = true;
  } catch (e) {
    console.error('Failed to load settings:', e);
  }
}

async function loadTokens() {
  try {
    const tokens = await API.get('/tokens');
    renderTokensList(tokens);
  } catch (e) {
    console.error('Failed to load tokens:', e);
  }
}

function renderTokensList(tokens) {
  const container = document.getElementById('tokens-list');
  if (!container) return;

  if (tokens.length === 0) {
    container.innerHTML = `
      <div class="empty-state tokens-empty">
        <p>No API tokens yet</p>
        <p class="text-muted">Generate a token to enable authenticated API access</p>
      </div>
    `;
    return;
  }

  container.innerHTML = tokens.map(token => `
    <div class="token-row">
      <div class="token-info">
        <span class="token-name">${escapeHtml(token.name)}</span>
        <span class="token-created">Created ${formatDate(token.created_at)}</span>
      </div>
      <div class="token-actions-row">
        <button class="btn btn-danger btn-sm" onclick="confirmDeleteToken('${token.id}', '${escapeHtml(token.name)}')">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M6.5 1.75a.25.25 0 01.25-.25h2.5a.25.25 0 01.25.25V3h-3V1.75zm4.5 0V3h2.25a.75.75 0 010 1.5H2.75a.75.75 0 010-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75zM4.496 6.675a.75.75 0 10-1.492.15l.66 6.6A1.75 1.75 0 005.405 15h5.19c.9 0 1.652-.681 1.741-1.576l.66-6.6a.75.75 0 00-1.492-.149l-.66 6.6a.25.25 0 01-.249.225h-5.19a.25.25 0 01-.249-.225l-.66-6.6z"/>
          </svg>
          Delete
        </button>
      </div>
    </div>
  `).join('');
}

function formatDate(dateStr) {
  try {
    const date = new Date(dateStr);
    return date.toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit'
    });
  } catch {
    return dateStr;
  }
}

window.showCreateTokenModal = () => {
  showModal('create-token', {
    title: 'Generate API Token'
  });
};

window.createToken = async () => {
  const nameInput = document.getElementById('token-name-input');
  const name = nameInput.value.trim();

  if (!name) {
    showToast('Please enter a token name', 'warning');
    return;
  }

  try {
    const result = await API.post('/tokens', { name });

    // Show the generated token
    showModal('token-created', {
      title: 'Token Generated',
      token: result.token
    });

    // Reload tokens list
    await loadTokens();
  } catch (e) {
    showToast(`Failed to create token: ${e.message}`, 'error');
  }
};

window.copyToken = (token) => {
  navigator.clipboard.writeText(token).then(() => {
    showToast('Token copied to clipboard', 'success');
  }).catch(() => {
    showToast('Failed to copy token', 'error');
  });
};

window.confirmDeleteToken = (id, name) => {
  showModal('confirm', {
    title: 'Delete Token',
    message: `Are you sure you want to delete the token <strong>${escapeHtml(name)}</strong>?<br><br>Any applications using this token will lose access.`,
    confirmText: 'Delete',
    confirmClass: 'btn-danger',
    onConfirm: () => deleteToken(id)
  });
};

async function deleteToken(id) {
  try {
    await API.delete(`/tokens/${id}`);
    showToast('Token deleted', 'success');
    hideModal();
    await loadTokens();
  } catch (e) {
    showToast(`Failed to delete token: ${e.message}`, 'error');
  }
}

// =============================================================================
// Console (REPL)
// =============================================================================

let consoleHistory = [];
let consoleHistoryIndex = -1;

window.onConsoleKeydown = (e) => {
  const input = e.target;

  if (e.key === 'Enter') {
    e.preventDefault();
    runConsoleCommand();
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    if (consoleHistoryIndex < consoleHistory.length - 1) {
      consoleHistoryIndex++;
      input.value = consoleHistory[consoleHistory.length - 1 - consoleHistoryIndex];
    }
  } else if (e.key === 'ArrowDown') {
    e.preventDefault();
    if (consoleHistoryIndex > 0) {
      consoleHistoryIndex--;
      input.value = consoleHistory[consoleHistory.length - 1 - consoleHistoryIndex];
    } else {
      consoleHistoryIndex = -1;
      input.value = '';
    }
  }
};

window.runConsoleCommand = async () => {
  const input = document.getElementById('console-input');
  const output = document.getElementById('console-output');
  const command = input.value.trim();

  if (!command) return;

  // Add to history
  consoleHistory.push(command);
  consoleHistoryIndex = -1;

  // Display the command
  appendConsoleEntry('command', `> ${command}`);

  // Clear input
  input.value = '';

  // Handle special commands
  if (command === '.help' || command === 'help') {
    appendConsoleEntry('result', `Available commands:
  .help              Show this help
  .clear             Clear console output
  .tables            List all tables
  .collections       Alias for .tables
  .schema <table>    Show table schema (document fields)
  .count <table>     Count documents in table

Query examples:
  db.table("users").run()
  db.table("users").filter(u => u.age > 21)
  db.table("posts").insert({title: "Hello"})
  db.table("users").get("uuid-here")
  db.table("users").delete("uuid-here")`);
    return;
  }

  if (command === '.clear') {
    clearConsole();
    return;
  }

  if (command === '.tables' || command === '.collections') {
    try {
      const cols = await API.get('/collections');
      if (cols.length === 0) {
        appendConsoleEntry('result', 'No tables found.');
      } else {
        const tableList = cols.map(c => `  ${c.name} (${c.count} docs)`).join('\n');
        appendConsoleEntry('result', `Tables:\n${tableList}`);
      }
    } catch (e) {
      appendConsoleEntry('error', `Error: ${e.message}`);
    }
    return;
  }

  if (command.startsWith('.schema ')) {
    const tableName = command.slice(8).trim();
    try {
      const docs = await API.get(`/collections/${encodeURIComponent(tableName)}?limit=10`);
      if (docs.length === 0) {
        appendConsoleEntry('result', `Table "${tableName}" is empty.`);
      } else {
        // Collect all unique keys from documents
        const keys = new Set();
        docs.forEach(d => {
          Object.keys(d.data || {}).forEach(k => keys.add(k));
        });
        const keyList = Array.from(keys).sort().map(k => `  - ${k}`).join('\n');
        appendConsoleEntry('result', `Fields in "${tableName}":\n${keyList || '  (no fields found)'}`);
      }
    } catch (e) {
      appendConsoleEntry('error', `Error: ${e.message}`);
    }
    return;
  }

  if (command.startsWith('.count ')) {
    const tableName = command.slice(7).trim();
    try {
      const cols = await API.get('/collections');
      const col = cols.find(c => c.name === tableName);
      if (col) {
        appendConsoleEntry('result', `${col.count} documents in "${tableName}"`);
      } else {
        appendConsoleEntry('error', `Table "${tableName}" not found.`);
      }
    } catch (e) {
      appendConsoleEntry('error', `Error: ${e.message}`);
    }
    return;
  }

  // Execute as query
  const start = performance.now();
  try {
    const result = await API.post('/query', { query: command });
    const elapsed = performance.now() - start;

    // Format result
    const formatted = JSON.stringify(result, null, 2);
    const count = Array.isArray(result) ? result.length : 1;
    appendConsoleEntry('result', formatted);
    appendConsoleEntry('info', `${count} result(s) in ${elapsed.toFixed(1)}ms`);

    // Refresh collections in case we modified data
    loadCollections();
  } catch (e) {
    appendConsoleEntry('error', `Error: ${e.message}`);
  }
};

function appendConsoleEntry(type, content) {
  const output = document.getElementById('console-output');
  if (!output) return;

  // Remove welcome message on first command
  const welcome = output.querySelector('.console-welcome');
  if (welcome) {
    welcome.remove();
  }

  const entry = document.createElement('div');
  entry.className = `console-entry console-${type}`;
  entry.innerHTML = `<pre>${escapeHtml(content)}</pre>`;
  output.appendChild(entry);

  // Scroll to bottom
  output.scrollTop = output.scrollHeight;
}

window.clearConsole = () => {
  const output = document.getElementById('console-output');
  if (!output) return;

  output.innerHTML = `
    <div class="console-welcome">
      <p class="console-help">Console cleared. Type <code>.help</code> for commands.</p>
    </div>
  `;
};

// =============================================================================
// Server Logs
// =============================================================================

let logSocket = null;
let logCount = 0;
let logAutoScroll = true;
let logLevelFilter = 'all';
let allLogs = [];

window.connectLogs = () => {
  if (logSocket && logSocket.readyState === WebSocket.OPEN) {
    return;
  }

  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  let wsUrl = `${protocol}//${window.location.host}/ws/logs`;

  // Add auth token if available
  const token = Auth.getToken();
  if (token) {
    wsUrl += `?token=${encodeURIComponent(token)}`;
  }

  try {
    logSocket = new WebSocket(wsUrl);

    logSocket.onopen = () => {
      updateLogConnectionStatus(true);
      showToast('Connected to log stream', 'success');

      // Clear empty state
      const container = document.getElementById('logs-container');
      if (container) {
        const emptyState = container.querySelector('.empty-state');
        if (emptyState) {
          emptyState.innerHTML = `
            <p class="text-muted">Waiting for logs...</p>
          `;
        }
      }
    };

    logSocket.onmessage = (event) => {
      try {
        const log = JSON.parse(event.data);
        addLogEntry(log);
      } catch (e) {
        // Plain text log
        addLogEntry({ level: 'info', message: event.data, timestamp: new Date().toISOString() });
      }
    };

    logSocket.onclose = () => {
      updateLogConnectionStatus(false);
      logSocket = null;
    };

    logSocket.onerror = (error) => {
      console.error('Log WebSocket error:', error);
      showToast('Log connection error', 'error');
      updateLogConnectionStatus(false);
    };

  } catch (e) {
    console.error('Failed to connect to logs:', e);
    showToast('Failed to connect: ' + e.message, 'error');
  }
};

window.disconnectLogs = () => {
  if (logSocket) {
    logSocket.close();
    logSocket = null;
  }
  updateLogConnectionStatus(false);
  showToast('Disconnected from log stream', 'success');
};

function updateLogConnectionStatus(connected) {
  const dot = document.getElementById('log-status-dot');
  const text = document.getElementById('log-status-text');
  const connectBtn = document.getElementById('log-connect-btn');
  const disconnectBtn = document.getElementById('log-disconnect-btn');

  if (dot) {
    dot.classList.toggle('disconnected', !connected);
  }
  if (text) {
    text.textContent = connected ? 'Connected' : 'Disconnected';
  }
  if (connectBtn) {
    connectBtn.disabled = connected;
  }
  if (disconnectBtn) {
    disconnectBtn.disabled = !connected;
  }
}

function addLogEntry(log) {
  const container = document.getElementById('logs-container');
  if (!container) return;

  // Clear empty state on first log
  const emptyState = container.querySelector('.empty-state');
  if (emptyState) {
    emptyState.remove();
  }

  // Store log
  allLogs.push(log);
  logCount++;

  // Check if log should be visible based on filter
  const level = (log.level || 'info').toLowerCase();
  const shouldShow = logLevelFilter === 'all' || level === logLevelFilter;

  if (shouldShow) {
    const entry = createLogElement(log);
    container.appendChild(entry);

    // Auto-scroll if enabled
    if (logAutoScroll) {
      container.scrollTop = container.scrollHeight;
    }
  }

  // Update count
  updateLogCount();

  // Limit stored logs
  if (allLogs.length > 1000) {
    allLogs.shift();
    const firstEntry = container.querySelector('.log-entry');
    if (firstEntry) {
      firstEntry.remove();
    }
  }
}

function createLogElement(log) {
  const level = (log.level || 'info').toLowerCase();
  const timestamp = log.timestamp ? new Date(log.timestamp).toLocaleTimeString() : '';
  const target = log.target || '';
  const message = log.message || log.msg || JSON.stringify(log);

  const entry = document.createElement('div');
  entry.className = `log-entry log-${level}`;
  entry.dataset.level = level;
  entry.innerHTML = `
    <span class="log-time">${timestamp}</span>
    <span class="log-level">${level.toUpperCase()}</span>
    ${target ? `<span class="log-target">${escapeHtml(target)}</span>` : ''}
    <span class="log-message">${escapeHtml(message)}</span>
  `;
  return entry;
}

function updateLogCount() {
  const countEl = document.getElementById('log-count');
  if (countEl) {
    countEl.textContent = `${logCount} log${logCount !== 1 ? 's' : ''}`;
  }
}

window.clearLogs = () => {
  const container = document.getElementById('logs-container');
  if (!container) return;

  allLogs = [];
  logCount = 0;
  updateLogCount();

  container.innerHTML = `
    <div class="empty-state">
      <p class="text-muted">${logSocket ? 'Waiting for logs...' : 'Click Connect to start streaming server logs'}</p>
    </div>
  `;
};

window.toggleAutoScroll = () => {
  const checkbox = document.getElementById('log-autoscroll');
  logAutoScroll = checkbox?.checked ?? true;
};

window.filterLogs = () => {
  const select = document.getElementById('log-level-filter');
  logLevelFilter = select?.value || 'all';

  // Re-render all logs with filter
  const container = document.getElementById('logs-container');
  if (!container) return;

  container.innerHTML = '';

  const filtered = allLogs.filter(log => {
    const level = (log.level || 'info').toLowerCase();
    return logLevelFilter === 'all' || level === logLevelFilter;
  });

  if (filtered.length === 0) {
    container.innerHTML = `
      <div class="empty-state">
        <p class="text-muted">No logs matching filter</p>
      </div>
    `;
    return;
  }

  filtered.forEach(log => {
    container.appendChild(createLogElement(log));
  });

  if (logAutoScroll) {
    container.scrollTop = container.scrollHeight;
  }
};
