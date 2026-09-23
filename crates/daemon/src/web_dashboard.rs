pub const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Craft Server Management Dashboard</title>
    <style>
        :root {
            --bg-base: #0f141c;
            --bg-surface: #182230;
            --bg-subtle: #223043;
            --border: #2d3f58;
            --text-primary: #e6edf3;
            --text-secondary: #8b9eb5;
            --accent-cyan: #38bdf8;
            --accent-green: #34d399;
            --accent-yellow: #fbbf24;
            --accent-red: #f87171;
            --accent-purple: #c084fc;
        }
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            background-color: var(--bg-base);
            color: var(--text-primary);
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            font-size: 14px;
            line-height: 1.5;
        }
        header {
            background-color: var(--bg-surface);
            border-bottom: 1px solid var(--border);
            padding: 12px 24px;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }
        .brand {
            display: flex;
            align-items: center;
            gap: 12px;
            font-weight: 700;
            font-size: 16px;
            letter-spacing: 0.5px;
            color: var(--accent-cyan);
        }
        .nav-tabs {
            display: flex;
            gap: 8px;
        }
        .nav-btn {
            background: transparent;
            border: 1px solid transparent;
            color: var(--text-secondary);
            padding: 6px 14px;
            border-radius: 4px;
            cursor: pointer;
            font-size: 13px;
            font-weight: 500;
            transition: all 0.15s ease;
        }
        .nav-btn:hover {
            color: var(--text-primary);
            background-color: var(--bg-subtle);
        }
        .nav-btn.active {
            color: var(--accent-cyan);
            background-color: var(--bg-subtle);
            border-color: var(--border);
        }
        .user-panel {
            display: flex;
            align-items: center;
            gap: 12px;
        }
        .badge {
            font-size: 11px;
            text-transform: uppercase;
            padding: 2px 8px;
            border-radius: 12px;
            font-weight: 600;
        }
        .badge-cyan { background: rgba(56, 189, 248, 0.15); color: var(--accent-cyan); }
        .badge-green { background: rgba(52, 211, 153, 0.15); color: var(--accent-green); }
        .badge-yellow { background: rgba(251, 191, 36, 0.15); color: var(--accent-yellow); }
        .badge-red { background: rgba(248, 113, 113, 0.15); color: var(--accent-red); }
        .container {
            max-width: 1280px;
            margin: 24px auto;
            padding: 0 24px;
        }
        .card {
            background: var(--bg-surface);
            border: 1px solid var(--border);
            border-radius: 6px;
            padding: 20px;
            margin-bottom: 20px;
        }
        .metrics-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 16px;
            margin-bottom: 20px;
        }
        .metric-card {
            background: var(--bg-surface);
            border: 1px solid var(--border);
            border-radius: 6px;
            padding: 16px;
        }
        .metric-title {
            color: var(--text-secondary);
            font-size: 12px;
            text-transform: uppercase;
            font-weight: 600;
        }
        .metric-value {
            font-size: 24px;
            font-weight: 700;
            margin-top: 4px;
            color: var(--text-primary);
        }
        table {
            width: 100%;
            border-collapse: collapse;
            text-align: left;
        }
        th, td {
            padding: 12px 16px;
            border-bottom: 1px solid var(--border);
        }
        th {
            background-color: var(--bg-subtle);
            color: var(--text-secondary);
            font-size: 12px;
            text-transform: uppercase;
            font-weight: 600;
        }
        tr:hover {
            background-color: rgba(255, 255, 255, 0.02);
        }
        .btn {
            background: var(--bg-subtle);
            color: var(--text-primary);
            border: 1px solid var(--border);
            padding: 5px 12px;
            border-radius: 4px;
            cursor: pointer;
            font-size: 12px;
            font-weight: 500;
            transition: all 0.15s ease;
        }
        .btn:hover {
            background: var(--border);
        }
        .btn-green { background: rgba(52, 211, 153, 0.15); color: var(--accent-green); border-color: rgba(52, 211, 153, 0.3); }
        .btn-green:hover { background: rgba(52, 211, 153, 0.25); }
        .btn-red { background: rgba(248, 113, 113, 0.15); color: var(--accent-red); border-color: rgba(248, 113, 113, 0.3); }
        .btn-red:hover { background: rgba(248, 113, 113, 0.25); }
        .btn-cyan { background: rgba(56, 189, 248, 0.15); color: var(--accent-cyan); border-color: rgba(56, 189, 248, 0.3); }
        .btn-cyan:hover { background: rgba(56, 189, 248, 0.25); }
        .console-box {
            background: #090d14;
            border: 1px solid var(--border);
            border-radius: 6px;
            padding: 12px;
            font-family: "JetBrains Mono", Menlo, Consolas, Monaco, monospace;
            font-size: 12px;
            height: 480px;
            overflow-y: auto;
            color: #d1d5db;
            white-space: pre-wrap;
            word-break: break-all;
        }
        .input-group {
            display: flex;
            gap: 8px;
            margin-top: 12px;
        }
        input, select {
            background: var(--bg-subtle);
            border: 1px solid var(--border);
            color: var(--text-primary);
            padding: 8px 12px;
            border-radius: 4px;
            font-size: 13px;
        }
        input:focus, select:focus {
            outline: none;
            border-color: var(--accent-cyan);
        }
        .auth-container {
            max-width: 380px;
            margin: 100px auto;
            background: var(--bg-surface);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 28px;
        }
        .tab-pane { display: none; }
        .tab-pane.active { display: block; }
    </style>
</head>
<body>
    <div id="auth-view" class="auth-container" style="display: none;">
        <h2 style="margin-bottom: 8px; color: var(--accent-cyan);">Craft Administrative Login</h2>
        <p style="color: var(--text-secondary); margin-bottom: 20px; font-size: 12px;">Multi-Tenant RBAC & Security Gateway</p>
        <form id="login-form">
            <div style="margin-bottom: 14px;">
                <label style="display:block; margin-bottom: 4px; font-size: 12px; color: var(--text-secondary);">Username</label>
                <input type="text" id="login-username" style="width: 100%;" required autocomplete="username">
            </div>
            <div style="margin-bottom: 18px;">
                <label style="display:block; margin-bottom: 4px; font-size: 12px; color: var(--text-secondary);">Password</label>
                <input type="password" id="login-password" style="width: 100%;" required autocomplete="current-password">
            </div>
            <button type="submit" class="btn btn-cyan" style="width: 100%; padding: 10px; font-size: 13px;">Authenticate</button>
            <div id="login-error" style="color: var(--accent-red); margin-top: 10px; font-size: 12px; display: none;"></div>
        </form>
    </div>

    <div id="app-view">
        <header>
            <div class="brand">
                <span>[Craft]</span>
                <span>Management Gateway</span>
            </div>
            <div class="nav-tabs">
                <button class="nav-btn active" onclick="switchTab('servers')">Servers</button>
                <button class="nav-btn" onclick="switchTab('console')">Live Console</button>
                <button class="nav-btn" onclick="switchTab('audit')">Audit Ledger</button>
                <button class="nav-btn" onclick="switchTab('users')">Access Control</button>
            </div>
            <div class="user-panel">
                <span id="user-display" style="font-weight: 500;">admin</span>
                <span id="role-badge" class="badge badge-cyan">SuperAdmin</span>
                <button class="btn btn-red" onclick="handleLogout()">Logout</button>
            </div>
        </header>

        <div class="container">
            <!-- Servers Tab -->
            <div id="tab-servers" class="tab-pane active">
                <div class="metrics-grid">
                    <div class="metric-card">
                        <div class="metric-title">Total Managed Servers</div>
                        <div class="metric-value" id="stat-total-servers">0</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-title">Active Running</div>
                        <div class="metric-value" style="color: var(--accent-green);" id="stat-running-servers">0</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-title">Sleeping / Hibernated</div>
                        <div class="metric-value" style="color: var(--accent-cyan);" id="stat-sleeping-servers">0</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-title">Daemon Uptime</div>
                        <div class="metric-value" id="stat-uptime">0s</div>
                    </div>
                </div>

                <div class="card">
                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;">
                        <h3 style="font-size: 16px; font-weight: 600;">Server Fleet Control</h3>
                        <button class="btn btn-cyan" onclick="loadServers()">Refresh Fleet</button>
                    </div>
                    <table>
                        <thead>
                            <tr>
                                <th>Name</th>
                                <th>Software</th>
                                <th>Port</th>
                                <th>Status</th>
                                <th>Actions</th>
                            </tr>
                        </thead>
                        <tbody id="servers-table-body">
                            <tr><td colspan="5" style="text-align: center; color: var(--text-secondary);">Loading server fleet...</td></tr>
                        </tbody>
                    </table>
                </div>
            </div>

            <!-- Console Tab -->
            <div id="tab-console" class="tab-pane">
                <div class="card">
                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;">
                        <div style="display: flex; align-items: center; gap: 12px;">
                            <h3 style="font-size: 16px; font-weight: 600;">Live Virtual Console</h3>
                            <select id="console-server-select" onchange="connectConsole()">
                                <option value="">Select a running server...</option>
                            </select>
                        </div>
                        <div style="display: flex; gap: 8px;">
                            <button class="btn" onclick="clearConsole()">Clear Window</button>
                            <span id="ws-status" class="badge badge-yellow">Disconnected</span>
                        </div>
                    </div>
                    <div id="console-output" class="console-box">Select a running server above to attach live console stream...</div>
                    <form id="console-form" class="input-group" onsubmit="sendConsoleCommand(event)">
                        <input type="text" id="console-input" placeholder="Type command (e.g. list, help, tps, op) and press Enter..." style="flex: 1;">
                        <button type="submit" class="btn btn-cyan">Execute</button>
                    </form>
                </div>
            </div>

            <!-- Audit Tab -->
            <div id="tab-audit" class="tab-pane">
                <div class="card">
                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;">
                        <div>
                            <h3 style="font-size: 16px; font-weight: 600;">Cryptographic Audit Ledger</h3>
                            <p style="font-size: 12px; color: var(--text-secondary);">Immutable hash-chained ledger verifying all administrative operations</p>
                        </div>
                        <div style="display: flex; gap: 8px;">
                            <button class="btn btn-cyan" onclick="verifyAuditLedger()">Verify Hash Chain</button>
                            <button class="btn" onclick="loadAuditLogs()">Refresh Log</button>
                        </div>
                    </div>
                    <div id="audit-verify-status" style="margin-bottom: 16px; display: none;"></div>
                    <table>
                        <thead>
                            <tr>
                                <th>Timestamp (UTC)</th>
                                <th>Actor</th>
                                <th>Action</th>
                                <th>Resource</th>
                                <th>Status</th>
                                <th>Entry Hash</th>
                            </tr>
                        </thead>
                        <tbody id="audit-table-body">
                            <tr><td colspan="6" style="text-align: center; color: var(--text-secondary);">Loading audit ledger...</td></tr>
                        </tbody>
                    </table>
                </div>
            </div>

            <!-- Users Tab -->
            <div id="tab-users" class="tab-pane">
                <div class="card">
                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;">
                        <div>
                            <h3 style="font-size: 16px; font-weight: 600;">Role-Based Access Control (RBAC)</h3>
                            <p style="font-size: 12px; color: var(--text-secondary);">Manage multi-tenant operator accounts, role boundaries, and server access scopes</p>
                        </div>
                        <button class="btn btn-cyan" onclick="loadUsers()">Refresh Accounts</button>
                    </div>
                    <table>
                        <thead>
                            <tr>
                                <th>Username</th>
                                <th>Role</th>
                                <th>Assigned Server Scope</th>
                                <th>Account Status</th>
                                <th>Created At</th>
                            </tr>
                        </thead>
                        <tbody id="users-table-body">
                            <tr><td colspan="5" style="text-align: center; color: var(--text-secondary);">Loading accounts...</td></tr>
                        </tbody>
                    </table>
                </div>
            </div>
        </div>
    </div>

    <script>
        let sessionToken = localStorage.getItem('craft_token') || '';
        let currentUser = null;
        let consoleSocket = null;

        function checkAuth() {
            if (!sessionToken) {
                document.getElementById('auth-view').style.display = 'block';
                document.getElementById('app-view').style.display = 'none';
            } else {
                document.getElementById('auth-view').style.display = 'none';
                document.getElementById('app-view').style.display = 'block';
                loadInitialData();
            }
        }

        document.getElementById('login-form').addEventListener('submit', async (e) => {
            e.preventDefault();
            const u = document.getElementById('login-username').value;
            const p = document.getElementById('login-password').value;
            const errBox = document.getElementById('login-error');
            errBox.style.display = 'none';

            try {
                const res = await fetch('/api/auth/login', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ username: u, password: p })
                });
                const data = await res.json();
                if (res.ok && data.token) {
                    sessionToken = data.token;
                    currentUser = data.user;
                    localStorage.setItem('craft_token', sessionToken);
                    localStorage.setItem('craft_user', JSON.stringify(currentUser));
                    checkAuth();
                } else {
                    errBox.textContent = data.error || 'Authentication failed';
                    errBox.style.display = 'block';
                }
            } catch (err) {
                errBox.textContent = 'Network error connecting to daemon gateway: ' + err.message;
                errBox.style.display = 'block';
            }
        });

        function handleLogout() {
            if (sessionToken) {
                fetch('/api/auth/logout', {
                    method: 'POST',
                    headers: { 'Authorization': 'Bearer ' + sessionToken }
                });
            }
            sessionToken = '';
            currentUser = null;
            localStorage.removeItem('craft_token');
            localStorage.removeItem('craft_user');
            if (consoleSocket) consoleSocket.close();
            checkAuth();
        }

        function switchTab(tabId) {
            document.querySelectorAll('.nav-btn').forEach(b => b.classList.remove('active'));
            document.querySelectorAll('.tab-pane').forEach(p => p.classList.remove('active'));
            event.target.classList.add('active');
            document.getElementById('tab-' + tabId).classList.add('active');

            if (tabId === 'servers') loadServers();
            if (tabId === 'audit') loadAuditLogs();
            if (tabId === 'users') loadUsers();
        }

        async function loadInitialData() {
            const stored = localStorage.getItem('craft_user');
            if (stored) {
                currentUser = JSON.parse(stored);
                document.getElementById('user-display').textContent = currentUser.username;
                document.getElementById('role-badge').textContent = currentUser.role;
            }
            loadServers();
        }

        async function loadServers() {
            try {
                const res = await fetch('/api/servers', {
                    headers: { 'Authorization': 'Bearer ' + sessionToken }
                });
                if (res.status === 401) return handleLogout();
                const data = await res.json();

                let runningCount = 0;
                let sleepingCount = 0;
                const tbody = document.getElementById('servers-table-body');
                const select = document.getElementById('console-server-select');
                tbody.innerHTML = '';
                select.innerHTML = '<option value="">Select a running server...</option>';

                data.servers.forEach(s => {
                    if (s.is_running) runningCount++;
                    if (s.is_sleeping) sleepingCount++;

                    if (s.is_running) {
                        const opt = document.createElement('option');
                        opt.value = s.name;
                        opt.textContent = s.name + ' (Port ' + s.port + ')';
                        select.appendChild(opt);
                    }

                    const tr = document.createElement('tr');
                    let statusBadge = '<span class="badge badge-red">[STOPPED]</span>';
                    if (s.is_running) statusBadge = '<span class="badge badge-green">[RUNNING]</span>';
                    if (s.is_sleeping) statusBadge = '<span class="badge badge-cyan">[SLEEPING]</span>';

                    tr.innerHTML = `
                        <td style="font-weight: 600; color: var(--accent-cyan);">${s.name}</td>
                        <td>${s.software} (${s.version})</td>
                        <td>${s.port}</td>
                        <td>${statusBadge}</td>
                        <td>
                            ${!s.is_running ? `<button class="btn btn-green" onclick="serverAction('${s.name}', 'start')">Start</button>` : ''}
                            ${s.is_running ? `<button class="btn btn-red" onclick="serverAction('${s.name}', 'stop')">Stop</button>` : ''}
                            ${s.is_running ? `<button class="btn" onclick="serverAction('${s.name}', 'restart')">Restart</button>` : ''}
                            ${s.is_running && !s.is_sleeping ? `<button class="btn btn-cyan" onclick="serverAction('${s.name}', 'hibernate')">Hibernate</button>` : ''}
                            ${s.is_sleeping ? `<button class="btn btn-green" onclick="serverAction('${s.name}', 'wake')">Wake</button>` : ''}
                        </td>
                    `;
                    tbody.appendChild(tr);
                });

                document.getElementById('stat-total-servers').textContent = data.servers.length;
                document.getElementById('stat-running-servers').textContent = runningCount;
                document.getElementById('stat-sleeping-servers').textContent = sleepingCount;
                document.getElementById('stat-uptime').textContent = data.uptime_seconds + 's';
            } catch (e) {
                console.error('Failed to load servers:', e);
            }
        }

        async function serverAction(name, action) {
            try {
                const res = await fetch(`/api/servers/${name}/${action}`, {
                    method: 'POST',
                    headers: { 'Authorization': 'Bearer ' + sessionToken }
                });
                const data = await res.json();
                if (!res.ok) alert('Action failed: ' + (data.error || res.statusText));
                loadServers();
            } catch (err) {
                alert('Network error: ' + err.message);
            }
        }

        function connectConsole() {
            const server = document.getElementById('console-server-select').value;
            const output = document.getElementById('console-output');
            const statusBadge = document.getElementById('ws-status');

            if (consoleSocket) {
                consoleSocket.close();
                consoleSocket = null;
            }

            if (!server) {
                output.textContent = 'Select a running server above to attach live console stream...';
                statusBadge.textContent = 'Disconnected';
                statusBadge.className = 'badge badge-yellow';
                return;
            }

            output.textContent = `[Craft Gateway] Connecting to live console feed for '${server}'...\n`;
            const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${proto}//${window.location.host}/ws/console?server=${encodeURIComponent(server)}&token=${encodeURIComponent(sessionToken)}`;

            consoleSocket = new WebSocket(wsUrl);
            consoleSocket.onopen = () => {
                statusBadge.textContent = 'Connected (Live)';
                statusBadge.className = 'badge badge-green';
                output.textContent += `[Craft Gateway] Attached to server '${server}' standard I/O stream.\n\n`;
            };

            consoleSocket.onmessage = (event) => {
                try {
                    const msg = JSON.parse(event.data);
                    if (msg.type === 'log') {
                        output.textContent += msg.data + '\n';
                        output.scrollTop = output.scrollHeight;
                    }
                } catch {
                    output.textContent += event.data + '\n';
                    output.scrollTop = output.scrollHeight;
                }
            };

            consoleSocket.onclose = () => {
                statusBadge.textContent = 'Closed';
                statusBadge.className = 'badge badge-red';
            };
        }

        function sendConsoleCommand(e) {
            e.preventDefault();
            const input = document.getElementById('console-input');
            const cmd = input.value.trim();
            if (!cmd || !consoleSocket || consoleSocket.readyState !== WebSocket.OPEN) return;
            consoleSocket.send(JSON.stringify({ command: cmd }));
            input.value = '';
        }

        function clearConsole() {
            document.getElementById('console-output').textContent = '';
        }

        async function loadAuditLogs() {
            try {
                const res = await fetch('/api/audit', {
                    headers: { 'Authorization': 'Bearer ' + sessionToken }
                });
                if (res.status === 401) return handleLogout();
                const data = await res.json();
                const tbody = document.getElementById('audit-table-body');
                tbody.innerHTML = '';

                if (!data.entries || data.entries.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="6" style="text-align: center; color: var(--text-secondary);">No audit records found</td></tr>';
                    return;
                }

                data.entries.reverse().forEach(e => {
                    const tr = document.createElement('tr');
                    let statusBadge = '<span class="badge badge-green">[SUCCESS]</span>';
                    if (e.status === 'DENIED') statusBadge = '<span class="badge badge-yellow">[DENIED]</span>';
                    if (e.status === 'FAILED') statusBadge = '<span class="badge badge-red">[FAILED]</span>';

                    tr.innerHTML = `
                        <td style="font-family: monospace; font-size: 11px;">${e.timestamp}</td>
                        <td style="font-weight: 600;">${e.actor}</td>
                        <td style="color: var(--accent-cyan); font-weight: 500;">${e.action}</td>
                        <td>${e.resource || '-'}</td>
                        <td>${statusBadge}</td>
                        <td style="font-family: monospace; font-size: 11px; color: var(--text-secondary);">${e.entry_hash.substring(0, 16)}...</td>
                    `;
                    tbody.appendChild(tr);
                });
            } catch (err) {
                console.error('Failed to load audit logs:', err);
            }
        }

        async function verifyAuditLedger() {
            const statusDiv = document.getElementById('audit-verify-status');
            statusDiv.style.display = 'block';
            statusDiv.innerHTML = '<span class="badge badge-yellow">Verifying continuous cryptographic hash chain...</span>';

            try {
                const res = await fetch('/api/audit/verify', {
                    headers: { 'Authorization': 'Bearer ' + sessionToken }
                });
                const data = await res.json();
                if (data.is_valid) {
                    statusDiv.innerHTML = `<span class="badge badge-green">[OK] Cryptographic Hash Chain Verified!</span> <span style="font-size: 12px; color: var(--text-secondary); margin-left: 8px;">All ${data.verified_entries} ledger entries intact with valid HMAC signatures.</span>`;
                } else {
                    statusDiv.innerHTML = `<span class="badge badge-red">[CORRUPTED] Chain Verification Failed!</span> <span style="font-size: 12px; color: var(--accent-red); margin-left: 8px;">${data.error_message || 'Tamper detected at index ' + data.corrupted_index}</span>`;
                }
            } catch (err) {
                statusDiv.innerHTML = `<span class="badge badge-red">[ERROR] Verification request failed: ${err.message}</span>`;
            }
        }

        async function loadUsers() {
            try {
                const res = await fetch('/api/rbac/users', {
                    headers: { 'Authorization': 'Bearer ' + sessionToken }
                });
                if (res.status === 401) return handleLogout();
                const data = await res.json();
                const tbody = document.getElementById('users-table-body');
                tbody.innerHTML = '';

                if (!data.users || data.users.length === 0) {
                    tbody.innerHTML = '<tr><td colspan="5" style="text-align: center; color: var(--text-secondary);">No user records found</td></tr>';
                    return;
                }

                data.users.forEach(u => {
                    const tr = document.createElement('tr');
                    tr.innerHTML = `
                        <td style="font-weight: 600; color: var(--accent-cyan);">${u.username}</td>
                        <td><span class="badge badge-cyan">${u.role}</span></td>
                        <td>${u.assigned_servers ? u.assigned_servers.join(', ') : 'All Servers (*)'}</td>
                        <td>${u.disabled ? '<span class="badge badge-red">Disabled</span>' : '<span class="badge badge-green">Active</span>'}</td>
                        <td style="font-size: 12px; color: var(--text-secondary);">${u.created_at}</td>
                    `;
                    tbody.appendChild(tr);
                });
            } catch (err) {
                console.error('Failed to load users:', err);
            }
        }

        // Initialize on page load
        checkAuth();
    </script>
</body>
</html>
"#;
