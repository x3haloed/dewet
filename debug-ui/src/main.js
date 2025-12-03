// Dewet Debug UI

// Import Tauri API (will be available when running in Tauri)
let invoke, listen;
try {
  const tauri = await import('@tauri-apps/api/core');
  const event = await import('@tauri-apps/api/event');
  invoke = tauri.invoke;
  listen = event.listen;
} catch (e) {
  // Running in browser without Tauri - create mock functions
  console.log('Running in browser mode (no Tauri)');
  invoke = async (cmd, args) => {
    console.log('Mock invoke:', cmd, args);
    return null;
  };
  listen = async (event, callback) => {
    console.log('Mock listen:', event);
    return () => {};
  };
}

// DOM Elements
const connectionStatus = document.getElementById('connection-status');
const decisionLog = document.getElementById('decision-log');
const logStream = document.getElementById('log-stream');
const screenPreview = document.getElementById('screen-preview');
const activeWindow = document.getElementById('active-window');
const activeApp = document.getElementById('active-app');
const visionAnalysis = document.getElementById('vision-analysis');

const characterSelect = document.getElementById('character-select');
const forceSpeakText = document.getElementById('force-speak-text');
const forceSpeakBtn = document.getElementById('force-speak-btn');
const resetCooldownsBtn = document.getElementById('reset-cooldowns-btn');
const daemonUrl = document.getElementById('daemon-url');
const reconnectBtn = document.getElementById('reconnect-btn');

// State
let connected = false;
let decisions = [];
let logs = [];
let visionHistory = [];

// Initialize
async function init() {
  // Set up event listeners
  setupControls();
  
  // Listen for daemon events
  await listen('daemon-event', (event) => {
    handleDaemonEvent(event.payload);
  });
  
  // Check initial connection status
  try {
    connected = await invoke('get_connection_status');
    updateConnectionStatus();
  } catch (e) {
    console.error('Failed to get connection status:', e);
  }
}

function setupControls() {
  forceSpeakBtn.disabled = true;
  forceSpeakBtn.title = 'Force speak controls coming soon';
  resetCooldownsBtn.disabled = true;
  resetCooldownsBtn.title = 'Cooldown management not yet available';

  forceSpeakBtn.addEventListener('click', async () => {
    const characterId = characterSelect.value;
    const text = forceSpeakText.value.trim() || null;
    
    try {
      await invoke('force_speak', { characterId, text });
      forceSpeakText.value = '';
    } catch (e) {
      console.error('Force speak failed:', e);
    }
  });
  
  resetCooldownsBtn.addEventListener('click', async () => {
    try {
      await invoke('reset_cooldowns');
    } catch (e) {
      console.error('Reset cooldowns failed:', e);
    }
  });
  
  reconnectBtn.addEventListener('click', async () => {
    const url = daemonUrl.value.trim();
    try {
      await invoke('connect_to_daemon', { url });
    } catch (e) {
      console.error('Reconnect failed:', e);
    }
  });
  
  // Log filter checkboxes
  document.querySelectorAll('.log-filters input[type="checkbox"]').forEach(checkbox => {
    checkbox.addEventListener('change', () => {
      filterLogs();
    });
  });
}

function handleDaemonEvent(event) {
  switch (event.type) {
    case 'connected':
      connected = true;
      updateConnectionStatus();
      break;
      
    case 'disconnected':
      connected = false;
      updateConnectionStatus();
      break;
      
    case 'arbiter_decision':
      addDecision(event);
      break;
      
    case 'vision_analysis':
      updateVisionAnalysis(event);
      break;
      
    case 'log':
      addLog(event);
      break;
      
    case 'screen_capture':
      updateScreenPreview(event);
      break;
      
    case 'speak':
      // Could highlight the speaking character
      break;
  }
}

function updateConnectionStatus() {
  connectionStatus.className = `status ${connected ? 'connected' : 'disconnected'}`;
}

function addDecision(decision) {
  decisions.unshift(decision);
  if (decisions.length > 50) decisions.pop();
  
  renderDecisions();
}

function renderDecisions() {
  if (decisions.length === 0) {
    decisionLog.innerHTML = '<p class="placeholder">Waiting for decisions...</p>';
    return;
  }
  
  decisionLog.innerHTML = decisions.map(d => `
    <div class="decision-entry ${d.should_respond ? 'responded' : 'passed'}">
      <div class="timestamp">${formatTime(d.timestamp)}</div>
      ${d.should_respond && d.responder_id 
        ? `<div class="responder">${d.responder_id}</div>` 
        : '<div class="responder" style="color: var(--text-muted)">Pass</div>'}
      <div class="reasoning">${escapeHtml(d.reasoning)}</div>
    </div>
  `).join('');
}

function addLog(log) {
  logs.unshift(log);
  if (logs.length > 200) logs.pop();
  
  filterLogs();
}

function filterLogs() {
  const activeFilters = new Set();
  document.querySelectorAll('.log-filters input:checked').forEach(cb => {
    activeFilters.add(cb.dataset.level);
  });
  
  const filteredLogs = logs.filter(l => activeFilters.has(l.level.toLowerCase()));
  
  if (filteredLogs.length === 0) {
    logStream.innerHTML = '<p class="placeholder">No logs matching filters</p>';
    return;
  }
  
  logStream.innerHTML = filteredLogs.map(l => `
    <div class="log-entry">
      <span class="level ${l.level.toLowerCase()}">${l.level}</span>
      <span class="message">${escapeHtml(l.message)}</span>
      <span class="time">${formatTime(l.timestamp)}</span>
    </div>
  `).join('');
}

function updateScreenPreview(data) {
  if (data.image_base64) {
    screenPreview.innerHTML = `<img src="data:image/png;base64,${data.image_base64}" alt="Screen capture">`;
  }
  
  activeWindow.textContent = data.active_window || '-';
  activeApp.textContent = data.active_app || '-';
}

function updateVisionAnalysis(data) {
  visionHistory.unshift(data);
  if (visionHistory.length > 20) visionHistory.pop();
  
  renderVisionAnalysis();
}

function renderVisionAnalysis() {
  if (visionHistory.length === 0) {
    visionAnalysis.innerHTML = '<p class="placeholder">Waiting for VLM analysis...</p>';
    return;
  }
  
  visionAnalysis.innerHTML = visionHistory.map(v => `
    <div class="vision-entry ${v.warrants_response ? 'active' : 'passive'}">
      <div class="timestamp">${formatTime(v.timestamp)}</div>
      <div class="activity">${escapeHtml(v.activity)}</div>
      <div class="meta">
        <span class="warrants ${v.warrants_response ? 'yes' : 'no'}">
          ${v.warrants_response ? '🟢 Response warranted' : '⚪ Passive'}
        </span>
        ${v.response_trigger ? `<span class="trigger">Trigger: ${escapeHtml(v.response_trigger)}</span>` : ''}
      </div>
      ${Object.keys(v.companion_interest || {}).length > 0 ? `
        <div class="interests">
          ${Object.entries(v.companion_interest).map(([id, score]) => 
            `<span class="interest-badge" style="opacity: ${0.3 + score * 0.7}">${id}: ${(score * 100).toFixed(0)}%</span>`
          ).join('')}
        </div>
      ` : ''}
    </div>
  `).join('');
}

function formatTime(timestamp) {
  const date = new Date(timestamp * 1000);
  return date.toLocaleTimeString();
}

function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

// Start
init();

