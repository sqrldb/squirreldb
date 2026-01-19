use leptos::*;

/// Main Admin application - renders static shell, JS handles interactivity
#[component]
pub fn App() -> impl IntoView {
  view! {
    <div class="app-container">
      <Sidebar/>
      <main class="content">
        <Dashboard/>
        <Tables/>
        <Explorer/>
        <Console/>
        <Live/>
        <Logs/>
        <Settings/>
      </main>
      <ModalContainer/>
      <ToastContainer/>
    </div>
  }
}

#[component]
fn Sidebar() -> impl IntoView {
  view! {
    <nav class="sidebar">
      <div class="logo">
        <h1>"SquirrelDB"</h1>
        <div class="theme-toggle">
          <button class="theme-btn" data-theme="light" onclick="setTheme('light')" title="Light mode">
            <svg viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 12a4 4 0 100-8 4 4 0 000 8zM8 0a.75.75 0 01.75.75v1.5a.75.75 0 01-1.5 0V.75A.75.75 0 018 0zm0 13a.75.75 0 01.75.75v1.5a.75.75 0 01-1.5 0v-1.5A.75.75 0 018 13zM2.343 2.343a.75.75 0 011.061 0l1.06 1.061a.75.75 0 01-1.06 1.06l-1.06-1.06a.75.75 0 010-1.06zm9.193 9.193a.75.75 0 011.06 0l1.061 1.06a.75.75 0 01-1.06 1.061l-1.061-1.06a.75.75 0 010-1.061zM0 8a.75.75 0 01.75-.75h1.5a.75.75 0 010 1.5H.75A.75.75 0 010 8zm13 0a.75.75 0 01.75-.75h1.5a.75.75 0 010 1.5h-1.5A.75.75 0 0113 8zM2.343 13.657a.75.75 0 010-1.061l1.06-1.06a.75.75 0 111.061 1.06l-1.06 1.06a.75.75 0 01-1.061 0zm9.193-9.193a.75.75 0 010-1.06l1.061-1.061a.75.75 0 111.06 1.06l-1.06 1.061a.75.75 0 01-1.061 0z"/>
            </svg>
          </button>
          <button class="theme-btn" data-theme="dark" onclick="setTheme('dark')" title="Dark mode">
            <svg viewBox="0 0 16 16" fill="currentColor">
              <path d="M6.2 1.5a.65.65 0 01.39.86 5.5 5.5 0 007.05 7.05.65.65 0 01.86.39.65.65 0 01-.2.72 7.5 7.5 0 11-8.77-8.77.65.65 0 01.67-.25z"/>
            </svg>
          </button>
          <button class="theme-btn active" data-theme="system" onclick="setTheme('system')" title="System preference">
            <svg viewBox="0 0 16 16" fill="currentColor">
              <path d="M2 3.5A1.5 1.5 0 013.5 2h9A1.5 1.5 0 0114 3.5v7a1.5 1.5 0 01-1.5 1.5h-9A1.5 1.5 0 012 10.5v-7zM3.5 3a.5.5 0 00-.5.5v7a.5.5 0 00.5.5h9a.5.5 0 00.5-.5v-7a.5.5 0 00-.5-.5h-9z"/>
              <path d="M5 14h6v-1H5v1z"/>
            </svg>
          </button>
        </div>
      </div>
      <div class="server-status">
        <span class="status-indicator" id="server-status-dot"></span>
        <span id="server-status-text">"Connected"</span>
      </div>
      <div class="nav-section">
        <ul class="nav-links">
          <li>
            <a href="#" class="nav-link active" data-page="dashboard" onclick="showPage('dashboard')">
              <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
                <path d="M6 1H2.5A1.5 1.5 0 001 2.5V6a1.5 1.5 0 001.5 1.5H6A1.5 1.5 0 007.5 6V2.5A1.5 1.5 0 006 1zm0 1.5v3H2.5v-3H6zM6 9H2.5A1.5 1.5 0 001 10.5V14a1.5 1.5 0 001.5 1.5H6A1.5 1.5 0 007.5 14v-3.5A1.5 1.5 0 006 9zm0 1.5v3H2.5v-3H6zM10 1h3.5A1.5 1.5 0 0115 2.5V6a1.5 1.5 0 01-1.5 1.5H10A1.5 1.5 0 018.5 6V2.5A1.5 1.5 0 0110 1zm0 1.5v3h3.5v-3H10zM10 9h3.5A1.5 1.5 0 0115 10.5V14a1.5 1.5 0 01-1.5 1.5H10A1.5 1.5 0 018.5 14v-3.5A1.5 1.5 0 0110 9zm0 1.5v3h3.5v-3H10z"/>
              </svg>
              "Dashboard"
            </a>
          </li>
          <li>
            <a href="#" class="nav-link" data-page="tables" onclick="showPage('tables')">
              <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
                <path d="M0 2a2 2 0 012-2h12a2 2 0 012 2v12a2 2 0 01-2 2H2a2 2 0 01-2-2V2zm14.5 0a.5.5 0 00-.5-.5H2a.5.5 0 00-.5.5v3h13V2zm0 4.5h-13v3h13v-3zm0 4.5h-13V14a.5.5 0 00.5.5h12a.5.5 0 00.5-.5v-3z"/>
              </svg>
              "Tables"
            </a>
          </li>
          <li>
            <a href="#" class="nav-link" data-page="explorer" onclick="showPage('explorer')">
              <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
                <path d="M14 4.5V14a2 2 0 01-2 2H4a2 2 0 01-2-2V2a2 2 0 012-2h5.5L14 4.5zM3 1a1 1 0 00-1 1v12a1 1 0 001 1h10a1 1 0 001-1V5h-3.5A1.5 1.5 0 019 3.5V1H4a1 1 0 00-1 0zM10 1v2.5a.5.5 0 00.5.5H13l-3-3zM4.5 8a.5.5 0 000 1h7a.5.5 0 000-1h-7zm0 2.5a.5.5 0 000 1h7a.5.5 0 000-1h-7zm0 2.5a.5.5 0 000 1h4a.5.5 0 000-1h-4z"/>
              </svg>
              "Explorer"
            </a>
          </li>
          <li>
            <a href="#" class="nav-link" data-page="console" onclick="showPage('console')">
              <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
                <path d="M6 9a.5.5 0 01.5-.5h3a.5.5 0 010 1h-3A.5.5 0 016 9zM3.854 4.146a.5.5 0 10-.708.708L4.793 6.5 3.146 8.146a.5.5 0 10.708.708l2-2a.5.5 0 000-.708l-2-2z"/>
                <path d="M2 1a2 2 0 00-2 2v10a2 2 0 002 2h12a2 2 0 002-2V3a2 2 0 00-2-2H2zm12 1a1 1 0 011 1v10a1 1 0 01-1 1H2a1 1 0 01-1-1V3a1 1 0 011-1h12z"/>
              </svg>
              "Console"
            </a>
          </li>
          <li>
            <a href="#" class="nav-link" data-page="live" onclick="showPage('live')">
              <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
                <path d="M3.05 3.05a7 7 0 000 9.9.5.5 0 01-.707.707 8 8 0 010-11.314.5.5 0 01.707.707zm9.9-.707a.5.5 0 01.707 0 8 8 0 010 11.314.5.5 0 01-.707-.707 7 7 0 000-9.9.5.5 0 010-.707zM4.464 4.465a.5.5 0 01-.707 0 5 5 0 000 7.072.5.5 0 01-.707.707 6 6 0 010-8.486.5.5 0 01.707.707zm7.072-.707a6 6 0 010 8.486.5.5 0 01-.707-.707 5 5 0 000-7.072.5.5 0 11.707-.707zM8 10a2 2 0 100-4 2 2 0 000 4z"/>
              </svg>
              "Live"
            </a>
          </li>
          <li>
            <a href="#" class="nav-link" data-page="logs" onclick="showPage('logs')">
              <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
                <path d="M5 4a.5.5 0 000 1h6a.5.5 0 000-1H5zm-.5 2.5A.5.5 0 015 6h6a.5.5 0 010 1H5a.5.5 0 01-.5-.5zM5 8a.5.5 0 000 1h6a.5.5 0 000-1H5zm0 2a.5.5 0 000 1h3a.5.5 0 000-1H5z"/>
                <path d="M2 2a2 2 0 012-2h8a2 2 0 012 2v12a2 2 0 01-2 2H4a2 2 0 01-2-2V2zm10-1H4a1 1 0 00-1 1v12a1 1 0 001 1h8a1 1 0 001-1V2a1 1 0 00-1-1z"/>
              </svg>
              "Logs"
            </a>
          </li>
          <li>
            <a href="#" class="nav-link" data-page="settings" onclick="showPage('settings')">
              <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
                <path fill-rule="evenodd" d="M7.429 1.525a6.593 6.593 0 011.142 0c.036.003.108.036.137.146l.289 1.105c.147.56.55.967.997 1.189.174.086.341.183.501.29.417.278.97.423 1.53.27l1.102-.303c.11-.03.175.016.195.046.219.31.41.641.573.989.014.031.022.11-.059.19l-.815.806c-.411.406-.562.957-.53 1.456a4.588 4.588 0 010 .582c-.032.499.119 1.05.53 1.456l.815.806c.08.08.073.159.059.19a6.494 6.494 0 01-.573.99c-.02.029-.086.074-.195.045l-1.103-.303c-.559-.153-1.112-.008-1.529.27-.16.107-.327.204-.5.29-.449.222-.851.628-.998 1.189l-.289 1.105c-.029.11-.101.143-.137.146a6.613 6.613 0 01-1.142 0c-.036-.003-.108-.037-.137-.146l-.289-1.105c-.147-.56-.55-.967-.997-1.189a4.502 4.502 0 01-.501-.29c-.417-.278-.97-.423-1.53-.27l-1.102.303c-.11.03-.175-.016-.195-.046a6.492 6.492 0 01-.573-.989c-.014-.031-.022-.11.059-.19l.815-.806c.411-.406.562-.957.53-1.456a4.587 4.587 0 010-.582c.032-.499-.119-1.05-.53-1.456l-.815-.806c-.08-.08-.073-.159-.059-.19a6.44 6.44 0 01.573-.99c.02-.029.086-.074.195-.045l1.103.303c.559.153 1.112.008 1.529-.27.16-.107.327-.204.5-.29.449-.222.851-.628.998-1.189l.289-1.105c.029-.11.101-.143.137-.146zM8 0c-.236 0-.47.01-.701.03-.743.065-1.29.615-1.458 1.261l-.29 1.106c-.017.066-.078.158-.211.224a5.994 5.994 0 00-.668.386c-.123.082-.233.09-.3.071L3.27 2.776c-.644-.177-1.392.02-1.82.63a7.977 7.977 0 00-.704 1.217c-.315.675-.111 1.422.363 1.891l.815.806c.05.048.098.147.088.294a6.084 6.084 0 000 .772c.01.147-.038.246-.088.294l-.815.806c-.474.469-.678 1.216-.363 1.891.2.428.436.835.704 1.218.428.609 1.176.806 1.82.63l1.103-.303c.066-.019.176-.011.299.071.213.143.436.272.668.386.133.066.194.158.212.224l.289 1.106c.169.646.715 1.196 1.458 1.26a8.094 8.094 0 001.402 0c.743-.064 1.29-.614 1.458-1.26l.29-1.106c.017-.066.078-.158.211-.224a5.98 5.98 0 00.668-.386c.123-.082.233-.09.3-.071l1.102.302c.644.177 1.392-.02 1.82-.63.268-.382.505-.789.704-1.217.315-.675.111-1.422-.364-1.891l-.814-.806c-.05-.048-.098-.147-.088-.294a6.1 6.1 0 000-.772c-.01-.147.039-.246.088-.294l.814-.806c.475-.469.679-1.216.364-1.891a7.992 7.992 0 00-.704-1.218c-.428-.609-1.176-.806-1.82-.63l-1.103.303c-.066.019-.176.011-.299-.071a5.991 5.991 0 00-.668-.386c-.133-.066-.194-.158-.212-.224L10.16 1.29C9.99.645 9.444.095 8.701.031A8.094 8.094 0 008 0zm1.5 8a1.5 1.5 0 11-3 0 1.5 1.5 0 013 0zM11 8a3 3 0 11-6 0 3 3 0 016 0z"/>
              </svg>
              "Settings"
            </a>
          </li>
        </ul>
      </div>
      <div class="table-quick-list">
        <h4>
          "Tables"
          <button onclick="createTable()" title="Create new table">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 2a.75.75 0 01.75.75v4.5h4.5a.75.75 0 010 1.5h-4.5v4.5a.75.75 0 01-1.5 0v-4.5h-4.5a.75.75 0 010-1.5h4.5v-4.5A.75.75 0 018 2z"/>
            </svg>
          </button>
        </h4>
        <ul id="sidebar-tables"></ul>
      </div>
      <div class="sidebar-footer">
        <div class="sidebar-footer-info" id="sidebar-footer-info">"v0.1.0"</div>
      </div>
    </nav>
  }
}

#[component]
fn Dashboard() -> impl IntoView {
  view! {
    <section id="dashboard" class="page active">
      <div class="page-header">
        <h2>"Dashboard"</h2>
        <div class="page-header-actions">
          <button class="btn btn-primary" onclick="createTable()">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 2a.75.75 0 01.75.75v4.5h4.5a.75.75 0 010 1.5h-4.5v4.5a.75.75 0 01-1.5 0v-4.5h-4.5a.75.75 0 010-1.5h4.5v-4.5A.75.75 0 018 2z"/>
            </svg>
            "New Table"
          </button>
        </div>
      </div>
      <div class="stats-grid">
        <div class="stat-card">
          <div class="stat-value" id="stat-tables">"-"</div>
          <div class="stat-label">"Tables"</div>
        </div>
        <div class="stat-card">
          <div class="stat-value" id="stat-documents">"-"</div>
          <div class="stat-label">"Documents"</div>
        </div>
        <div class="stat-card">
          <div class="stat-value" id="stat-backend">"-"</div>
          <div class="stat-label">"Backend"</div>
        </div>
        <div class="stat-card">
          <div class="stat-value" id="stat-uptime">"-"</div>
          <div class="stat-label">"Uptime"</div>
        </div>
      </div>
      <div class="tables-overview">
        <div class="section-header">
          <h3>"Tables"</h3>
        </div>
        <table class="data-table">
          <thead>
            <tr>
              <th>"Name"</th>
              <th>"Documents"</th>
              <th style="text-align: right">"Actions"</th>
            </tr>
          </thead>
          <tbody id="tables-list"></tbody>
        </table>
      </div>
    </section>
  }
}

#[component]
fn Tables() -> impl IntoView {
  view! {
    <section id="tables" class="page">
      <div class="page-header">
        <h2>"Tables"</h2>
        <div class="page-header-actions">
          <button class="btn btn-primary" onclick="createTable()">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 2a.75.75 0 01.75.75v4.5h4.5a.75.75 0 010 1.5h-4.5v4.5a.75.75 0 01-1.5 0v-4.5h-4.5a.75.75 0 010-1.5h4.5v-4.5A.75.75 0 018 2z"/>
            </svg>
            "New Table"
          </button>
        </div>
      </div>
      <div class="table-browser">
        <div class="table-data-panel">
          <div class="table-header">
            <h3>
              <select id="table-selector" class="table-selector" onchange="onTableSelect(this.value)">
                <option value="">"Select a table..."</option>
              </select>
            </h3>
            <div class="table-actions">
              <button class="btn btn-primary" onclick="insertDoc()" id="insert-btn" disabled>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M8 2a.75.75 0 01.75.75v4.5h4.5a.75.75 0 010 1.5h-4.5v4.5a.75.75 0 01-1.5 0v-4.5h-4.5a.75.75 0 010-1.5h4.5v-4.5A.75.75 0 018 2z"/>
                </svg>
                "Insert"
              </button>
              <button class="btn btn-secondary" onclick="exportTable()" id="export-btn" disabled>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M8.75 1a.75.75 0 00-1.5 0v6.44L5.03 5.22a.75.75 0 00-1.06 1.06l3.5 3.5a.75.75 0 001.06 0l3.5-3.5a.75.75 0 00-1.06-1.06L8.75 7.44V1z"/>
                  <path d="M1 11.75a.75.75 0 011.5 0v2.5a.25.25 0 00.25.25h10.5a.25.25 0 00.25-.25v-2.5a.75.75 0 011.5 0v2.5A1.75 1.75 0 0113.25 16H2.75A1.75 1.75 0 011 14.25v-2.5z"/>
                </svg>
                "Export"
              </button>
              <button class="btn btn-secondary" onclick="refreshTable()" id="refresh-btn" disabled>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M8 3a5 5 0 104.546 2.914.75.75 0 011.36-.636A6.5 6.5 0 118 1.5v2A.75.75 0 018 3z"/>
                  <path d="M8 1.5a.75.75 0 01.75.75v3.5a.75.75 0 01-1.5 0v-3.5A.75.75 0 018 1.5z"/>
                  <path d="M5.22 4.22a.75.75 0 011.06 0l2.5 2.5a.75.75 0 01-1.06 1.06l-2.5-2.5a.75.75 0 010-1.06z"/>
                </svg>
                "Refresh"
              </button>
              <div class="dropdown">
                <button class="btn btn-ghost" onclick="toggleTableMenu()" id="table-menu-btn" disabled>
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M3 9.5a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm5 0a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm5 0a1.5 1.5 0 110-3 1.5 1.5 0 010 3z"/>
                  </svg>
                </button>
                <div class="dropdown-menu" id="table-menu">
                  <div class="dropdown-item" onclick="importToTable()">
                    <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                      <path d="M8.75 15a.75.75 0 01-1.5 0V8.56L5.03 10.78a.75.75 0 01-1.06-1.06l3.5-3.5a.75.75 0 011.06 0l3.5 3.5a.75.75 0 01-1.06 1.06L8.75 8.56V15z"/>
                      <path d="M1 4.25a.75.75 0 011.5 0v-2.5a.25.25 0 01.25-.25h10.5a.25.25 0 01.25.25v2.5a.75.75 0 011.5 0v-2.5A1.75 1.75 0 0013.25 0H2.75A1.75 1.75 0 001 1.75v2.5z"/>
                    </svg>
                    "Import JSON"
                  </div>
                  <div class="dropdown-divider"></div>
                  <div class="dropdown-item danger" onclick="clearTable()">
                    <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                      <path d="M6.5 1.75a.25.25 0 01.25-.25h2.5a.25.25 0 01.25.25V3h-3V1.75zm4.5 0V3h2.25a.75.75 0 010 1.5H2.75a.75.75 0 010-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75zM4.496 6.675a.75.75 0 10-1.492.15l.66 6.6A1.75 1.75 0 005.405 15h5.19c.9 0 1.652-.681 1.741-1.576l.66-6.6a.75.75 0 00-1.492-.149l-.66 6.6a.25.25 0 01-.249.225h-5.19a.25.25 0 01-.249-.225l-.66-6.6z"/>
                    </svg>
                    "Clear All Data"
                  </div>
                </div>
              </div>
            </div>
          </div>
          <div class="documents-container">
            <div id="documents-grid" class="documents-grid">
              <div class="empty-state">
                <p>"Select a table to view documents"</p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  }
}

#[component]
fn Explorer() -> impl IntoView {
  view! {
    <section id="explorer" class="page">
      <h2>"Data Explorer"</h2>
      <div class="explorer-container">
        <div class="query-panel">
          <textarea
            id="query-input"
            class="query-textarea"
            placeholder="db.table(\"users\").run()"
          ></textarea>
          <div class="query-actions">
            <button class="btn btn-primary" onclick="runQuery()">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                <path d="M4 2l10 6-10 6V2z"/>
              </svg>
              "Run Query"
            </button>
            <span id="query-time" class="text-secondary"></span>
          </div>
        </div>
        <div class="results-panel">
          <h3>"Results"</h3>
          <pre id="query-results" class="results-content">"Enter a query and click Run to see results."</pre>
        </div>
      </div>
    </section>
  }
}

#[component]
fn Live() -> impl IntoView {
  view! {
    <section id="live" class="page">
      <div class="page-header">
        <h2>"Live Changes"</h2>
        <div class="page-header-actions">
          <div class="live-status" id="live-status">
            <span class="status-indicator disconnected" id="ws-status-dot"></span>
            <span id="ws-status-text">"Disconnected"</span>
          </div>
        </div>
      </div>

      <div class="live-controls">
        <div class="live-control-row">
          <div class="live-control-group">
            <label>"Subscribe to table"</label>
            <select id="live-table-selector" class="table-selector" onchange="onLiveTableChange()">
              <option value="">"All tables"</option>
            </select>
          </div>
          <div class="live-control-group">
            <label>"Filter"</label>
            <input type="text" id="live-filter" class="input" placeholder="e.g., doc => doc.status === 'active'" />
          </div>
          <div class="live-control-buttons">
            <button class="btn btn-primary" onclick="connectLive()" id="live-connect-btn">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                <path d="M3.05 3.05a7 7 0 000 9.9.5.5 0 01-.707.707 8 8 0 010-11.314.5.5 0 01.707.707z"/>
                <path d="M4.464 4.465a.5.5 0 01-.707 0 5 5 0 000 7.072.5.5 0 01-.707.707 6 6 0 010-8.486.5.5 0 01.707.707z"/>
                <path d="M8 10a2 2 0 100-4 2 2 0 000 4z"/>
              </svg>
              "Connect"
            </button>
            <button class="btn btn-secondary" onclick="disconnectLive()" id="live-disconnect-btn" disabled>
              <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                <path d="M4 4l8 8M4 12l8-8" stroke="currentColor" stroke-width="1.5" fill="none"/>
              </svg>
              "Disconnect"
            </button>
            <button class="btn btn-ghost" onclick="clearLiveFeed()">
              "Clear"
            </button>
          </div>
        </div>
      </div>

      <div class="live-stats">
        <div class="live-stat">
          <span class="live-stat-value" id="live-stat-total">"0"</span>
          <span class="live-stat-label">"Total"</span>
        </div>
        <div class="live-stat insert">
          <span class="live-stat-value" id="live-stat-insert">"0"</span>
          <span class="live-stat-label">"Inserts"</span>
        </div>
        <div class="live-stat update">
          <span class="live-stat-value" id="live-stat-update">"0"</span>
          <span class="live-stat-label">"Updates"</span>
        </div>
        <div class="live-stat delete">
          <span class="live-stat-value" id="live-stat-delete">"0"</span>
          <span class="live-stat-label">"Deletes"</span>
        </div>
      </div>

      <div class="live-feed-container">
        <div class="live-feed" id="live-feed">
          <div class="empty-state">
            <svg width="48" height="48" viewBox="0 0 16 16" fill="currentColor" style="opacity: 0.2">
              <path d="M3.05 3.05a7 7 0 000 9.9.5.5 0 01-.707.707 8 8 0 010-11.314.5.5 0 01.707.707zm9.9-.707a.5.5 0 01.707 0 8 8 0 010 11.314.5.5 0 01-.707-.707 7 7 0 000-9.9.5.5 0 010-.707z"/>
              <path d="M4.464 4.465a.5.5 0 01-.707 0 5 5 0 000 7.072.5.5 0 01-.707.707 6 6 0 010-8.486.5.5 0 01.707.707zm7.072-.707a6 6 0 010 8.486.5.5 0 01-.707-.707 5 5 0 000-7.072.5.5 0 11.707-.707z"/>
              <path d="M8 10a2 2 0 100-4 2 2 0 000 4z"/>
            </svg>
            <p>"Click Connect to start watching for changes"</p>
            <p class="text-muted">"Changes will appear here in real-time"</p>
          </div>
        </div>
      </div>
    </section>
  }
}

#[component]
fn Console() -> impl IntoView {
  view! {
    <section id="console" class="page">
      <div class="page-header">
        <h2>"Console"</h2>
        <div class="page-header-actions">
          <button class="btn btn-secondary" onclick="clearConsole()">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M6.5 1.75a.25.25 0 01.25-.25h2.5a.25.25 0 01.25.25V3h-3V1.75zm4.5 0V3h2.25a.75.75 0 010 1.5H2.75a.75.75 0 010-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75zM4.496 6.675a.75.75 0 10-1.492.15l.66 6.6A1.75 1.75 0 005.405 15h5.19c.9 0 1.652-.681 1.741-1.576l.66-6.6a.75.75 0 00-1.492-.149l-.66 6.6a.25.25 0 01-.249.225h-5.19a.25.25 0 01-.249-.225l-.66-6.6z"/>
            </svg>
            "Clear"
          </button>
        </div>
      </div>

      <div class="console-container">
        <div class="console-output" id="console-output">
          <div class="console-welcome">
            <pre class="ascii-logo">"  ____              _               _ ____  ____  \n / ___|  __ _ _   _(_)_ __ _ __ ___| |  _ \\| __ ) \n \\___ \\ / _` | | | | | '__| '__/ _ \\ | | | |  _ \\ \n  ___) | (_| | |_| | | |  | | |  __/ | |_| | |_) |\n |____/ \\__, |\\__,_|_|_|  |_|  \\___|_|____/|____/ \n           |_|"</pre>
            <p class="console-help">"Type JavaScript queries to interact with the database."</p>
            <p class="console-help">"Examples:"</p>
            <pre class="console-examples">"db.table(\"users\").run()                    // List all users\ndb.table(\"users\").filter(u => u.age > 21)  // Filter users\ndb.table(\"posts\").insert({title: \"Hi\"})    // Insert document\n.help                                       // Show all commands"</pre>
          </div>
        </div>
        <div class="console-input-container">
          <span class="console-prompt">">"</span>
          <input
            type="text"
            id="console-input"
            class="console-input"
            placeholder="Enter query..."
            autocomplete="off"
            spellcheck="false"
            onkeydown="onConsoleKeydown(event)"
          />
          <button class="btn btn-primary console-run-btn" onclick="runConsoleCommand()">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M4 2l10 6-10 6V2z"/>
            </svg>
          </button>
        </div>
      </div>
    </section>
  }
}

#[component]
fn Logs() -> impl IntoView {
  view! {
    <section id="logs" class="page">
      <div class="page-header">
        <h2>"Server Logs"</h2>
        <div class="page-header-actions">
          <div class="log-controls">
            <label class="log-control-checkbox">
              <input type="checkbox" id="log-autoscroll" checked onchange="toggleAutoScroll()" />
              <span>"Auto-scroll"</span>
            </label>
            <select id="log-level-filter" class="select-small" onchange="filterLogs()">
              <option value="all">"All levels"</option>
              <option value="error">"Error"</option>
              <option value="warn">"Warn"</option>
              <option value="info">"Info"</option>
              <option value="debug">"Debug"</option>
              <option value="trace">"Trace"</option>
            </select>
          </div>
          <button class="btn btn-secondary" onclick="clearLogs()">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M6.5 1.75a.25.25 0 01.25-.25h2.5a.25.25 0 01.25.25V3h-3V1.75zm4.5 0V3h2.25a.75.75 0 010 1.5H2.75a.75.75 0 010-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75zM4.496 6.675a.75.75 0 10-1.492.15l.66 6.6A1.75 1.75 0 005.405 15h5.19c.9 0 1.652-.681 1.741-1.576l.66-6.6a.75.75 0 00-1.492-.149l-.66 6.6a.25.25 0 01-.249.225h-5.19a.25.25 0 01-.249-.225l-.66-6.6z"/>
            </svg>
            "Clear"
          </button>
        </div>
      </div>

      <div class="log-status-bar">
        <div class="log-connection-status">
          <span class="status-indicator disconnected" id="log-status-dot"></span>
          <span id="log-status-text">"Disconnected"</span>
        </div>
        <div class="log-actions">
          <button class="btn btn-primary btn-sm" onclick="connectLogs()" id="log-connect-btn">
            "Connect"
          </button>
          <button class="btn btn-secondary btn-sm" onclick="disconnectLogs()" id="log-disconnect-btn" disabled>
            "Disconnect"
          </button>
        </div>
        <div class="log-stats">
          <span id="log-count">"0 logs"</span>
        </div>
      </div>

      <div class="logs-container" id="logs-container">
        <div class="empty-state">
          <svg width="48" height="48" viewBox="0 0 16 16" fill="currentColor" style="opacity: 0.2">
            <path d="M5 4a.5.5 0 000 1h6a.5.5 0 000-1H5zm-.5 2.5A.5.5 0 015 6h6a.5.5 0 010 1H5a.5.5 0 01-.5-.5zM5 8a.5.5 0 000 1h6a.5.5 0 000-1H5zm0 2a.5.5 0 000 1h3a.5.5 0 000-1H5z"/>
            <path d="M2 2a2 2 0 012-2h8a2 2 0 012 2v12a2 2 0 01-2 2H4a2 2 0 01-2-2V2zm10-1H4a1 1 0 00-1 1v12a1 1 0 001 1h8a1 1 0 001-1V2a1 1 0 00-1-1z"/>
          </svg>
          <p>"Click Connect to start streaming server logs"</p>
          <p class="text-muted">"Logs will appear here in real-time"</p>
        </div>
      </div>
    </section>
  }
}

#[component]
fn Settings() -> impl IntoView {
  view! {
    <section id="settings" class="page">
      <div class="page-header">
        <h2>"Settings"</h2>
      </div>

      <div class="settings-grid">
        // Protocols Card
        <div class="settings-card">
          <div class="settings-card-header">
            <h3>"Protocols"</h3>
            <span class="settings-card-description">"Enable or disable server protocols"</span>
          </div>
          <div class="settings-card-body">
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"REST API"</span>
                <span class="setting-description">"HTTP REST endpoints for CRUD operations"</span>
              </div>
              <label class="toggle">
                <input type="checkbox" id="setting-rest" checked disabled />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"WebSocket"</span>
                <span class="setting-description">"Real-time subscriptions and queries"</span>
              </div>
              <label class="toggle">
                <input type="checkbox" id="setting-websocket" checked disabled />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"Server-Sent Events"</span>
                <span class="setting-description">"Coming soon"</span>
              </div>
              <label class="toggle">
                <input type="checkbox" id="setting-sse" disabled />
                <span class="toggle-slider"></span>
              </label>
            </div>
          </div>
          <div class="settings-card-footer">
            <span class="text-muted">"Protocol changes require server restart"</span>
          </div>
        </div>

        // Authentication Card
        <div class="settings-card">
          <div class="settings-card-header">
            <h3>"Authentication"</h3>
            <span class="settings-card-description">"Configure API authentication"</span>
          </div>
          <div class="settings-card-body">
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"Enable Authentication"</span>
                <span class="setting-description">"Require API tokens for access"</span>
              </div>
              <label class="toggle">
                <input type="checkbox" id="setting-auth-enabled" disabled />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div id="auth-warning" class="setting-warning">
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <path d="M8.982 1.566a1.13 1.13 0 00-1.96 0L.165 13.233c-.457.778.091 1.767.98 1.767h13.713c.889 0 1.438-.99.98-1.767L8.982 1.566zM8 5c.535 0 .954.462.9.995l-.35 3.507a.552.552 0 01-1.1 0L7.1 5.995A.905.905 0 018 5zm.002 6a1 1 0 110 2 1 1 0 010-2z"/>
              </svg>
              <span>"Authentication is disabled. API is publicly accessible."</span>
            </div>
          </div>
          <div class="settings-card-footer">
            <span class="text-muted">"Auth changes require server restart"</span>
          </div>
        </div>

        // API Tokens Card
        <div class="settings-card settings-card-wide">
          <div class="settings-card-header">
            <h3>"API Tokens"</h3>
            <span class="settings-card-description">"Manage API access tokens"</span>
          </div>
          <div class="settings-card-body">
            <div class="token-actions">
              <button class="btn btn-primary" onclick="showCreateTokenModal()">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M8 2a.75.75 0 01.75.75v4.5h4.5a.75.75 0 010 1.5h-4.5v4.5a.75.75 0 01-1.5 0v-4.5h-4.5a.75.75 0 010-1.5h4.5v-4.5A.75.75 0 018 2z"/>
                </svg>
                "Generate Token"
              </button>
            </div>
            <div id="tokens-list" class="tokens-list">
              <div class="empty-state tokens-empty">
                <p>"No API tokens yet"</p>
                <p class="text-muted">"Generate a token to enable authenticated API access"</p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  }
}

#[component]
fn ModalContainer() -> impl IntoView {
  view! {
    <div id="modal-overlay" class="modal-overlay" onclick="onModalOverlayClick(event)">
      <div class="modal" onclick="event.stopPropagation()">
        <div class="modal-header">
          <h3 id="modal-title">"Modal"</h3>
          <button class="modal-close" onclick="hideModal()">"×"</button>
        </div>
        <div class="modal-body" id="modal-body"></div>
        <div class="modal-footer" id="modal-footer"></div>
      </div>
    </div>
  }
}

#[component]
fn ToastContainer() -> impl IntoView {
  view! {
    <div id="toast-container" class="toast-container"></div>
  }
}
