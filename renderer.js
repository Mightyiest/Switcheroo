// State management
let state = {
  system: null,
  accounts: [],
  masterSteamId3: null,
  autoSyncOnSwitch: true,
  launchOptions: '-novid -high -map dota',
  backups: [],
  isTestMode: true
};

const DUMMY_ACCOUNTS = [
  {
    steamId64: '76561198242661728',
    steamId3: '282396000',
    accountName: 'pinkyoshino',
    personaName: 'impasta (Main)',
    rememberPassword: true,
    isActive: true,
    avatar: null,
    hasDotaConfig: true,
    hotkeysHash: 'hash-master-01',
    dotaFilesCount: 18,
    launchOptions: '-novid -high -map dota'
  },
  {
    steamId64: '76561198142526674',
    steamId3: '182260946',
    accountName: 'owningz2',
    personaName: 't1 fekar P4',
    rememberPassword: true,
    isActive: false,
    avatar: null,
    hasDotaConfig: true,
    hotkeysHash: 'hash-master-01',
    dotaFilesCount: 18,
    launchOptions: ''
  },
  {
    steamId64: '76561198170051426',
    steamId3: '209785698',
    accountName: 'masterdeadly1',
    personaName: 'Yuēhàn 约翰 (Ranked)',
    rememberPassword: true,
    isActive: false,
    avatar: null,
    hasDotaConfig: true,
    hotkeysHash: 'hash-alt-unsynced-02',
    dotaFilesCount: 14,
    launchOptions: ''
  },
  {
    steamId64: '76561198274424993',
    steamId3: '314159265',
    accountName: 'miracle_god_mid',
    personaName: 'Miracle- Fanboy 👑',
    rememberPassword: true,
    isActive: false,
    avatar: null,
    hasDotaConfig: true,
    hotkeysHash: 'hash-master-01',
    dotaFilesCount: 18,
    launchOptions: ''
  },
  {
    steamId64: '76561198380399428',
    steamId3: '420133700',
    accountName: 'support_only_plz',
    personaName: 'Wards & Smokes (Pos 5)',
    rememberPassword: false,
    isActive: false,
    avatar: null,
    hasDotaConfig: true,
    hotkeysHash: 'hash-alt-unsynced-03',
    dotaFilesCount: 12,
    launchOptions: ''
  },
  {
    steamId64: '76561198516153851',
    steamId3: '555888123',
    accountName: 'immortal_grind_smurf',
    personaName: 'Road to Immortal ⚔️',
    rememberPassword: true,
    isActive: false,
    avatar: null,
    hasDotaConfig: false,
    hotkeysHash: null,
    dotaFilesCount: 0,
    launchOptions: ''
  }
];

// DOM Elements
const steamPathValue = document.getElementById('steamPathValue');
const steamDot = document.getElementById('steamDot');
const dotaPathValue = document.getElementById('dotaPathValue');
const dotaDot = document.getElementById('dotaDot');
const accountCount = document.getElementById('accountCount');
const accountsGrid = document.getElementById('accountsGrid');
const masterAccountName = document.getElementById('masterAccountName');
const masterBanner = document.getElementById('masterBanner');
const chkAutoSync = document.getElementById('chkAutoSync');
const txtLaunchOptions = document.getElementById('txtLaunchOptions');
const btnSyncAll = document.getElementById('btnSyncAll');
const btnBackups = document.getElementById('btnBackups');
const btnRefresh = document.getElementById('btnRefresh');
const btnWinMinimize = document.getElementById('btnWinMinimize');
const btnWinClose = document.getElementById('btnWinClose');
const btnToggleDummy = document.getElementById('btnToggleDummy');
const dummyIcon = document.getElementById('dummyIcon');
const dummyText = document.getElementById('dummyText');
const backupsModal = document.getElementById('backupsModal');
const btnCloseBackupsModal = document.getElementById('btnCloseBackupsModal');
const btnCloseBackupsModalBtn = document.getElementById('btnCloseBackupsModalBtn');
const btnOpenBackupsDir = document.getElementById('btnOpenBackupsDir');
const backupsList = document.getElementById('backupsList');
const confirmModal = document.getElementById('confirmModal');
const confirmTitle = document.getElementById('confirmTitle');
const confirmMessage = document.getElementById('confirmMessage');
const btnConfirmYes = document.getElementById('btnConfirmYes');
const btnConfirmNo = document.getElementById('btnConfirmNo');
const btnCancelConfirm = document.getElementById('btnCancelConfirm');
const toastContainer = document.getElementById('toastContainer');
const linkOpenUserdata = document.getElementById('linkOpenUserdata');
const linkOpenBackups = document.getElementById('linkOpenBackups');
const linkAbout = document.getElementById('linkAbout');
const brandHeader = document.getElementById('brandHeader');
const aboutModal = document.getElementById('aboutModal');
const btnCloseAboutModal = document.getElementById('btnCloseAboutModal');
const btnCloseAboutBtn = document.getElementById('btnCloseAboutBtn');

let pendingConfirmAction = null;
let isOperating = false;

function setBusy(busy) {
  isOperating = busy;
  const elements = document.querySelectorAll('button, input, a');
  elements.forEach(el => {
    if (el.closest('.win-controls')) return;
    if (busy) {
      el.style.pointerEvents = 'none';
      el.classList.add('is-disabled');
    } else {
      el.style.pointerEvents = '';
      el.classList.remove('is-disabled');
    }
  });
}

// Initialize App
document.addEventListener('DOMContentLoaded', async () => {
  setupEventListeners();
  await refreshAll();
});

function setupEventListeners() {
  btnRefresh.addEventListener('click', async () => {
    btnRefresh.style.transform = 'rotate(360deg)';
    btnRefresh.style.transition = 'transform 0.5s ease';
    setTimeout(() => { btnRefresh.style.transform = 'none'; }, 500);
    await refreshAll();
    showToast('Accounts and system status refreshed', 'info');
  });

  btnSyncAll.addEventListener('click', handleSyncAllClick);
  btnBackups.addEventListener('click', openBackupsModal);
  btnCloseBackupsModal.addEventListener('click', closeBackupsModal);
  btnCloseBackupsModalBtn.addEventListener('click', closeBackupsModal);
  btnOpenBackupsDir.addEventListener('click', async () => {
    const res = await window.switcheroo.openFolder();
    if (res && !res.success) showToast(res.error || 'Could not open backups folder.', 'error');
  });

  linkOpenUserdata.addEventListener('click', async (e) => {
    e.preventDefault();
    if (state.system && state.system.steam && state.system.steam.steamPath) {
      const res = await window.switcheroo.openFolder(state.system.steam.steamPath + '\\userdata');
      if (res && !res.success) showToast(res.error || 'Could not open userdata folder.', 'error');
    }
  });

  linkOpenBackups.addEventListener('click', async (e) => {
    e.preventDefault();
    const res = await window.switcheroo.openFolder();
    if (res && !res.success) showToast(res.error || 'Could not open backups folder.', 'error');
  });

  // Auto-Sync Toggle listener
  chkAutoSync.addEventListener('change', async (e) => {
    state.autoSyncOnSwitch = e.target.checked;
    await window.switcheroo.saveSettings({ autoSyncOnSwitch: state.autoSyncOnSwitch });
    showToast(`Auto-sync on switch is now ${state.autoSyncOnSwitch ? 'ENABLED' : 'DISABLED'}`, 'info');
  });

  // Launch options input listener with debounce
  let launchOptionsDebounce = null;
  txtLaunchOptions.addEventListener('input', (e) => {
    state.launchOptions = e.target.value.trim();
    clearTimeout(launchOptionsDebounce);
    launchOptionsDebounce = setTimeout(async () => {
      await window.switcheroo.saveSettings({ launchOptions: state.launchOptions });
    }, 300);
  });

  function hasLaunchOption(fullOptions, option) {
    if (!fullOptions || !option) return false;
    const escaped = option.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const regex = new RegExp(`(^|\\s)${escaped}(\\s|$)`);
    return regex.test(fullOptions);
  }

  // Launch option chips
  document.querySelectorAll('.chip-preset').forEach(chip => {
    chip.addEventListener('click', async () => {
      const val = chip.getAttribute('data-chip');
      let current = txtLaunchOptions.value.trim();
      if (!hasLaunchOption(current, val)) {
        current = current ? `${current} ${val}` : val;
        txtLaunchOptions.value = current;
        state.launchOptions = current;
        await window.switcheroo.saveSettings({ launchOptions: current });
        showToast(`Added ${val} to launch options`, 'info');
      }
    });
  });

  // Custom Frameless Window controls (Minimize & Close for fixed window)
  if (btnWinMinimize) {
    btnWinMinimize.addEventListener('click', () => {
      window.switcheroo.minimizeWindow();
    });
  }
  if (btnWinClose) {
    btnWinClose.addEventListener('click', () => {
      window.switcheroo.closeWindow();
    });
  }

  // Test Mode Toggle
  if (btnToggleDummy) {
    btnToggleDummy.addEventListener('click', async () => {
      state.isTestMode = !state.isTestMode;
      await refreshAll();
      showToast(
        state.isTestMode ? 'Loaded 6 dummy accounts for UI testing' : 'Loaded real Steam accounts',
        'info'
      );
    });
  }

  // Confirm Modal listeners
  btnConfirmYes.addEventListener('click', () => {
    if (pendingConfirmAction) pendingConfirmAction();
    closeConfirmModal();
  });
  btnConfirmNo.addEventListener('click', closeConfirmModal);
  btnCancelConfirm.addEventListener('click', closeConfirmModal);

  // About Modal listeners
  if (linkAbout) {
    linkAbout.addEventListener('click', (e) => {
      e.preventDefault();
      openAboutModal();
    });
  }
  if (brandHeader) {
    brandHeader.addEventListener('click', () => {
      openAboutModal();
    });
  }
  if (btnCloseAboutModal) btnCloseAboutModal.addEventListener('click', closeAboutModal);
  if (btnCloseAboutBtn) btnCloseAboutBtn.addEventListener('click', closeAboutModal);

  // External Links: Open in default browser
  document.querySelectorAll('.external-link').forEach(link => {
    link.addEventListener('click', (e) => {
      e.preventDefault();
      const href = link.getAttribute('href');
      if (href) {
        if (window.switcheroo.openExternalUrl) {
          window.switcheroo.openExternalUrl(href);
        } else {
          window.switcheroo.openSteamUrl(href);
        }
      }
    });
  });

  // Global ESC key listener to close modals
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      closeAboutModal();
      closeBackupsModal();
      closeConfirmModal();
    }
  });

  // Click background backdrop to close modals
  [backupsModal, confirmModal, aboutModal].forEach(modal => {
    if (modal) {
      modal.addEventListener('click', (e) => {
        if (e.target === modal) {
          modal.classList.add('hidden');
        }
      });
    }
  });
}

function openAboutModal() {
  if (aboutModal) aboutModal.classList.remove('hidden');
}

function closeAboutModal() {
  if (aboutModal) aboutModal.classList.add('hidden');
}

/**
 * Refreshes system detection, config, and accounts
 */
async function refreshAll() {
  try {
    // 1. Fetch system detection
    const system = await window.switcheroo.getSystemStatus();
    state.system = system;
    updateSystemUI(system);

    // 2. Fetch config settings
    const settings = await window.switcheroo.getSettings();
    if (settings) {
      if (settings.masterSteamId3) state.masterSteamId3 = settings.masterSteamId3;
      if (settings.autoSyncOnSwitch !== undefined) {
        state.autoSyncOnSwitch = settings.autoSyncOnSwitch;
        chkAutoSync.checked = state.autoSyncOnSwitch;
      }
      if (settings.launchOptions !== undefined) {
        state.launchOptions = settings.launchOptions;
        txtLaunchOptions.value = state.launchOptions;
      }
    }

    // 3. Fetch accounts (supports test mode with 6 accounts)
    let accounts = [];
    if (state.isTestMode) {
      accounts = DUMMY_ACCOUNTS.map(a => ({ ...a }));
      if (btnToggleDummy) {
        btnToggleDummy.classList.add('btn-test-active');
        if (dummyText) dummyText.textContent = 'Test Mode (6 Accounts)';
        if (dummyIcon) dummyIcon.textContent = '🧪';
      }
    } else {
      accounts = await window.switcheroo.getAccounts();
      if (btnToggleDummy) {
        btnToggleDummy.classList.remove('btn-test-active');
        if (dummyText) dummyText.textContent = 'Load Test (6 Accounts)';
        if (dummyIcon) dummyIcon.textContent = '👥';
      }
    }
    state.accounts = accounts;

    // If master is not set or not found, auto-select the active account or first account
    if ((!state.masterSteamId3 || !accounts.some(a => a.steamId3 === state.masterSteamId3)) && accounts.length > 0) {
      const active = accounts.find(a => a.isActive);
      state.masterSteamId3 = active ? active.steamId3 : accounts[0].steamId3;
      if (!state.isTestMode) {
        await window.switcheroo.saveSettings({ masterSteamId3: state.masterSteamId3 });
      }
    }

    renderAccounts();
    updateMasterBanner();
  } catch (err) {
    console.error('Refresh error:', err);
    showToast(`Error initializing: ${err.message}`, 'error');
  }
}

/**
 * Updates Steam & Dota 2 status pills
 */
function updateSystemUI(system) {
  if (system.steam && system.steam.found) {
    steamDot.className = 'status-dot active';
    steamPathValue.textContent = system.steam.steamPath;
    steamPathValue.title = system.steam.steamPath;
  } else {
    steamDot.className = 'status-dot';
    steamPathValue.textContent = 'Not Found';
    steamPathValue.title = 'Steam installation not found in registry or default paths';
  }

  if (system.dota && system.dota.installed) {
    dotaDot.className = 'status-dot active';
    dotaPathValue.textContent = 'Installed & Ready';
    dotaPathValue.title = system.dota.path;
  } else {
    dotaDot.className = 'status-dot';
    dotaPathValue.textContent = 'Not Found';
    dotaPathValue.title = 'Dota 2 not detected in Steam library folders';
  }
}

/**
 * Updates the top Master Template banner
 */
function updateMasterBanner() {
  const masterAcc = state.accounts.find(a => a.steamId3 === state.masterSteamId3);
  if (masterAcc) {
    masterAccountName.textContent = masterAcc.personaName;
    if (masterBanner) {
      masterBanner.title = `Master Template Account: ${masterAcc.personaName} (@${masterAcc.accountName}) • Dota 2 hotkeys & settings source`;
    }
  } else {
    masterAccountName.textContent = 'None Selected';
    if (masterBanner) {
      masterBanner.title = 'No master template selected. Click "Set as Master" on an account card.';
    }
  }
}

/**
 * Renders the accounts grid
 */
function renderAccounts() {
  accountCount.textContent = state.accounts.length;
  accountsGrid.innerHTML = '';

  if (state.accounts.length === 0) {
    accountsGrid.innerHTML = `
      <div class="loading-state">
        <p>No Steam accounts found in loginusers.vdf.</p>
        <span class="section-hint">Log into Steam on this computer at least once to register an account.</span>
      </div>
    `;
    return;
  }

  const masterAcc = state.accounts.find(a => a.steamId3 === state.masterSteamId3);

  state.accounts.forEach(acc => {
    const isMaster = acc.steamId3 === state.masterSteamId3;
    const card = document.createElement('div');
    card.className = `account-card ${acc.isActive ? 'is-active' : ''} ${isMaster ? 'is-master' : ''}`;

    // Avatar
    let avatarHtml = '';
    if (acc.avatar) {
      avatarHtml = `<img class="avatar-img" src="${acc.avatar}" alt="${escapeHtml(acc.personaName)}" />`;
    } else {
      const initial = (acc.personaName || acc.accountName || 'S').charAt(0).toUpperCase();
      avatarHtml = `<div class="avatar-fallback">${initial}</div>`;
    }

    // Keybind comparison with master
    let syncStatusHtml = '';
    if (isMaster) {
      syncStatusHtml = `
        <div class="meta-pill ready" title="Master template source">
          <span>★</span>
          <span>Source of Truth (${acc.dotaFilesCount} files)</span>
        </div>
      `;
    } else if (masterAcc && acc.hasDotaConfig) {
      const isInSync = Boolean(acc.hotkeysHash) && acc.hotkeysHash === masterAcc.hotkeysHash;
      if (isInSync) {
        syncStatusHtml = `
          <div class="meta-pill ready" title="Hotkeys match Master profile exactly">
            <span>✓</span>
            <span>Hotkeys In Sync</span>
          </div>
        `;
      } else {
        syncStatusHtml = `
          <div class="meta-pill warn" title="Keybind files differ from Master">
            <span>⚠️</span>
            <span>Unsynced Binds</span>
          </div>
        `;
      }
    } else {
      syncStatusHtml = `
        <div class="meta-pill warn">
          <span>⚠️</span>
          <span>No Dota Config</span>
        </div>
      `;
    }



    card.innerHTML = `
      <div class="account-header">
        <div class="avatar-wrapper">
          ${avatarHtml}
        </div>
        <div class="account-identity">
          <div class="persona-name" title="${escapeHtml(acc.personaName)}">${escapeHtml(acc.personaName)}</div>
          <div class="account-name">@${escapeHtml(acc.accountName)} <span class="steam-id">(${acc.steamId3})</span></div>
        </div>
      </div>

      <div class="account-meta">
        ${acc.isActive ? '<span class="meta-pill active-pill">● Active</span>' : ''}
        ${isMaster ? '<span class="meta-pill warn" style="color:var(--dota-gold);border-color:rgba(245,158,11,0.3)">★ Master</span>' : ''}
        ${syncStatusHtml}
      </div>

      <div class="account-actions">
        ${acc.isActive ? `
          <button class="btn-primary-action btn-active-play" data-action="launch" data-name="${escapeHtml(acc.accountName)}" data-id="${acc.steamId3}" title="Launch Dota 2 immediately without restarting Steam">
            ⚔️ Launch Dota 2
          </button>
        ` : `
          <button class="btn-primary-action btn-alt-play" data-action="launch" data-name="${escapeHtml(acc.accountName)}" data-id="${acc.steamId3}" title="Sync Master settings, switch to this account, and launch Dota 2 in 1 click">
            ⚡ Switch & Play Dota
          </button>
        `}

        <div class="card-sub-actions">
          <button class="btn-sub-card ${isMaster ? 'is-master' : ''}" data-action="set-master" data-id="${acc.steamId3}" title="Designate as Master template">
            ${isMaster ? '★ Master Template' : '☆ Make Master'}
          </button>
          
          ${!isMaster ? `
            <button class="btn-sub-card" data-action="sync-single" data-id="${acc.steamId3}" data-name="${escapeHtml(acc.accountName)}" title="Sync settings without launching">
              ⚡ Sync Only
            </button>
          ` : ''}

          <button class="btn-sub-card" data-action="open-folder" data-id="${acc.steamId3}" title="Open userdata folder in Explorer">
            📁 Folder
          </button>
        </div>
      </div>
    `;

    // Bind card actions
    card.querySelector('[data-action="set-master"]').addEventListener('click', () => setMasterAccount(acc.steamId3));
    
    const btnSync = card.querySelector('[data-action="sync-single"]');
    if (btnSync) {
      btnSync.addEventListener('click', () => handleSingleSync(acc));
    }

    card.querySelector('[data-action="launch"]').addEventListener('click', () => handleSwitchAccount(acc.accountName, acc.steamId3, true));
    card.querySelector('[data-action="open-folder"]').addEventListener('click', async () => {
      if (state.system && state.system.steam && state.system.steam.steamPath) {
        const targetPath = `${state.system.steam.steamPath}\\userdata\\${acc.steamId3}\\570`;
        const res = await window.switcheroo.openFolder(targetPath);
        if (res && !res.success) {
          showToast(`Cannot open folder: ${res.error || 'Dota 2 folder not found for this account.'}`, 'error');
        }
      }
    });

    accountsGrid.appendChild(card);
  });
}

/**
 * Sets an account as the designated master template
 */
async function setMasterAccount(steamId3) {
  state.masterSteamId3 = steamId3;
  if (!state.isTestMode) {
    await window.switcheroo.saveSettings({ masterSteamId3: steamId3 });
  }
  renderAccounts();
  updateMasterBanner();
  const acc = state.accounts.find(a => a.steamId3 === steamId3);
  showToast(
    state.isTestMode ? `[Test Mode] Template master set to ${acc ? acc.personaName : steamId3}` : `Template master set to ${acc ? acc.personaName : steamId3}`,
    'info'
  );
}

/**
 * Handles single account manual sync from Master
 */
function handleSingleSync(targetAcc) {
  const masterAcc = state.accounts.find(a => a.steamId3 === state.masterSteamId3);
  if (!masterAcc) {
    showToast('Please select a Master Template account first.', 'error');
    return;
  }

  showConfirm(
    'Sync Dota 2 Settings',
    `Clone hotkeys, quickcast, chat wheels and control groups from <strong>${escapeHtml(masterAcc.personaName)}</strong> to <strong>${escapeHtml(targetAcc.personaName)}</strong>?<br><br>A full backup of existing settings will be created automatically before overwriting.`,
    async () => {
      if (state.isTestMode) {
        showToast(`[Test Mode] Synced settings from ${masterAcc.personaName} to ${targetAcc.personaName}!`, 'success');
        targetAcc.hotkeysHash = masterAcc.hotkeysHash;
        targetAcc.hasDotaConfig = true;
        renderAccounts();
        return;
      }

      if (isOperating) return;
      setBusy(true);
      try {
        showToast(`Syncing Dota 2 settings to ${targetAcc.personaName}...`, 'info');
        const res = await window.switcheroo.syncDotaSettings({
          sourceSteamId3: masterAcc.steamId3,
          targetSteamId3: targetAcc.steamId3,
          targetAccountName: targetAcc.accountName
        });

        if (res.success) {
          showToast(`Successfully synced Dota 2 settings to ${targetAcc.personaName}! (Backup created)`, 'success');
          await refreshAll();
        }
      } catch (err) {
        console.error('Sync error:', err);
        showToast(`Sync failed: ${err.message}`, 'error');
      } finally {
        setBusy(false);
      }
    }
  );
}

/**
 * Handles Sync to All accounts
 */
function handleSyncAllClick() {
  const masterAcc = state.accounts.find(a => a.steamId3 === state.masterSteamId3);
  if (!masterAcc) {
    showToast('Please select a Master Template account first.', 'error');
    return;
  }

  const targetCount = state.accounts.filter(a => a.steamId3 !== masterAcc.steamId3).length;
  if (targetCount === 0) {
    showToast('No other accounts to sync.', 'info');
    return;
  }

  showConfirm(
    'Sync Master to All Accounts',
    `Clone all Dota 2 hotkeys & settings from Master <strong>${escapeHtml(masterAcc.personaName)}</strong> to all <strong>${targetCount} other account(s)</strong>?<br><br>Backups of all accounts will be created automatically before syncing.`,
    async () => {
      if (state.isTestMode) {
        showToast(`[Test Mode] Synced Master settings to all ${targetCount} accounts!`, 'success');
        state.accounts.forEach(a => {
          a.hotkeysHash = masterAcc.hotkeysHash;
          a.hasDotaConfig = true;
        });
        renderAccounts();
        return;
      }

      if (isOperating) return;
      setBusy(true);
      try {
        showToast(`Syncing to ${targetCount} accounts...`, 'info');
        const res = await window.switcheroo.syncToAll({ sourceSteamId3: masterAcc.steamId3 });
        showToast(`Successfully synced to ${res.syncedCount} account(s)! Backups created.`, 'success');
        await refreshAll();
      } catch (err) {
        console.error('Sync all error:', err);
        showToast(`Sync all failed: ${err.message}`, 'error');
      } finally {
        setBusy(false);
      }
    }
  );
}

/**
 * Switches Steam account and launches Dota 2 (or launches directly if already active)
 */
async function handleSwitchAccount(accountName, targetSteamId3, launchDota = true) {
  if (state.isTestMode) {
    showToast(`[Test Mode] Switched active profile to @${accountName} and launched Dota 2!`, 'success');
    state.accounts.forEach(a => {
      a.isActive = (a.accountName.toLowerCase() === accountName.toLowerCase());
    });
    renderAccounts();
    return;
  }

  if (isOperating) return;
  setBusy(true);
  try {
    const isTargetActive = state.accounts.some(a => a.accountName.toLowerCase() === accountName.toLowerCase() && a.isActive);
    let actionText = '';
    if (isTargetActive) {
      actionText = 'Launching Dota 2 directly...';
    } else {
      actionText = 'Syncing settings, switching account & launching Dota 2...';
    }
    showToast(actionText, 'info');

    const res = await window.switcheroo.switchAccount({
      targetAccountName: accountName,
      targetSteamId3: targetSteamId3,
      sourceSteamId3: state.masterSteamId3,
      autoSyncSettings: !isTargetActive && Boolean(state.autoSyncOnSwitch),
      launchDota: launchDota,
      launchOptions: state.launchOptions
    });

    if (res.success) {
      let successText = '';
      if (res.alreadyActive) {
        successText = `Launched Dota 2 directly on @${accountName}!`;
      } else if (res.synced) {
        successText = `Settings synced! Switched to @${accountName} and launched Dota 2!`;
      } else {
        successText = `Switched to @${accountName} and launched Dota 2!`;
      }
      showToast(successText, 'success');
      setTimeout(refreshAll, 2500);
    }
  } catch (err) {
    console.error('Switch error:', err);
    showToast(`Failed: ${err.message}`, 'error');
  } finally {
    setBusy(false);
  }
}

/**
 * Opens Backups Modal
 */
async function openBackupsModal() {
  try {
    const backups = await window.switcheroo.listBackups();
    state.backups = backups;
    renderBackupsList(backups);
    backupsModal.classList.remove('hidden');
  } catch (err) {
    showToast(`Error listing backups: ${err.message}`, 'error');
  }
}

function closeBackupsModal() {
  backupsModal.classList.add('hidden');
}

function renderBackupsList(backups) {
  backupsList.innerHTML = '';
  if (backups.length === 0) {
    backupsList.innerHTML = `<div class="empty-backups">No backups created yet. Backups are saved automatically before any sync operation.</div>`;
    return;
  }

  backups.forEach(b => {
    const item = document.createElement('div');
    item.className = 'backup-item';
    const dateFormatted = new Date(b.created).toLocaleString();

    item.innerHTML = `
      <div class="backup-info">
        <span class="backup-title">${escapeHtml(b.name)}</span>
        <span class="backup-date">Created: ${dateFormatted}</span>
      </div>
      <button class="btn btn-secondary" style="font-size:0.78rem; padding: 6px 12px;" data-name="${escapeHtml(b.name)}">
        Restore
      </button>
    `;

    item.querySelector('button').addEventListener('click', () => {
      handleRestoreBackup(b.name);
    });

    backupsList.appendChild(item);
  });
}

async function handleRestoreBackup(backupName) {
  const accountPrefix = backupName.split('_backup_')[0];
  const targetAcc = state.accounts.find(a => 
    a.accountName.toLowerCase() === accountPrefix.toLowerCase() ||
    a.steamId3 === accountPrefix
  );

  if (!targetAcc) {
    showToast(`Could not match target account for ${backupName}`, 'error');
    return;
  }

  showConfirm(
    'Restore Settings Backup',
    `Restore settings snapshot <strong>${escapeHtml(backupName)}</strong> back to <strong>${escapeHtml(targetAcc.personaName)}</strong>?`,
    async () => {
      if (isOperating) return;
      setBusy(true);
      try {
        await window.switcheroo.restoreBackup({
          backupFolderName: backupName,
          targetSteamId3: targetAcc.steamId3
        });
        showToast(`Settings restored to ${targetAcc.personaName}!`, 'success');
        closeBackupsModal();
        await refreshAll();
      } catch (err) {
        showToast(`Restore failed: ${err.message}`, 'error');
      } finally {
        setBusy(false);
      }
    }
  );
}

/**
 * Confirmation dialog helper
 */
function showConfirm(title, messageHtml, onYes) {
  confirmTitle.textContent = title;
  confirmMessage.innerHTML = messageHtml;
  pendingConfirmAction = onYes;
  confirmModal.classList.remove('hidden');
}

function closeConfirmModal() {
  confirmModal.classList.add('hidden');
  pendingConfirmAction = null;
}

/**
 * Toast notifications
 */
function showToast(message, type = 'info', duration = 4000) {
  const toast = document.createElement('div');
  toast.className = `toast toast-${type}`;
  
  let icon = 'ℹ️';
  if (type === 'success') icon = '✅';
  if (type === 'error') icon = '❌';

  toast.innerHTML = `<span>${icon}</span><span>${escapeHtml(message)}</span>`;
  toastContainer.appendChild(toast);

  setTimeout(() => {
    toast.style.opacity = '0';
    toast.style.transform = 'translateY(10px)';
    setTimeout(() => toast.remove(), 250);
  }, duration);
}

function escapeHtml(str) {
  if (!str) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
