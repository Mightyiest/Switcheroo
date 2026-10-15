const { app, BrowserWindow, ipcMain, shell, screen } = require('electron');
const path = require('path');
const fs = require('fs');
const steamService = require('./steam-service');

app.name = 'Switcheroo';
if (process.platform === 'win32') {
  app.setAppUserModelId('Switcheroo.Dota2.Switcher');
}

// Strictly enforce only one running instance of the application
const gotTheLock = app.requestSingleInstanceLock();
let mainWindow = null;

if (!gotTheLock) {
  // Second instance immediately exits
  app.quit();
} else {
  // If user tries to open a second instance, bring existing window to the front
  app.on('second-instance', () => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.show();
      mainWindow.setAlwaysOnTop(true);
      mainWindow.focus();
      mainWindow.setAlwaysOnTop(false);
    }
  });

  const baseConfigDir = process.env.PORTABLE_EXECUTABLE_DIR || app.getPath('userData');
  const CONFIG_FILE = path.join(baseConfigDir, 'switcheroo-settings.json');

  function loadConfig() {
    try {
      if (fs.existsSync(CONFIG_FILE)) {
        return JSON.parse(fs.readFileSync(CONFIG_FILE, 'utf8'));
      }
    } catch (err) {
      console.error('Failed to read config:', err);
    }
    return {
      masterSteamId3: null,
      autoSyncOnSwitch: true,
      launchOptions: '-novid -high -map dota'
    };
  }

  function saveConfig(data) {
    try {
      const current = loadConfig();
      const updated = { ...current, ...data };
      const tmpFile = `${CONFIG_FILE}.${Date.now()}-${Math.random().toString(36).slice(2)}.tmp`;
      fs.writeFileSync(tmpFile, JSON.stringify(updated, null, 2), 'utf8');
      fs.renameSync(tmpFile, CONFIG_FILE);
      return true;
    } catch (err) {
      console.error('Failed to save config:', err);
      return false;
    }
  }

  function getAppIcon() {
    const icoPath = path.join(__dirname, 'icon.ico');
    const pngPath = path.join(__dirname, 'icon.png');
    const webpPath = path.join(__dirname, 'icon.webp');
    if (fs.existsSync(icoPath)) return icoPath;
    if (fs.existsSync(pngPath)) return pngPath;
    return webpPath;
  }

  function createWindow() {
    const iconFile = getAppIcon();

    mainWindow = new BrowserWindow({
      width: 800,
      height: 520,
      useContentSize: true,
      resizable: false,
      maximizable: false,
      fullscreenable: false,
      title: 'Switcheroo',
      icon: iconFile,
      frame: false,
      backgroundColor: '#0b0c13',
      webPreferences: {
        preload: path.join(__dirname, 'preload.js'),
        contextIsolation: true,
        nodeIntegration: false
      },
      autoHideMenuBar: true
    });

    mainWindow.on('closed', () => {
      mainWindow = null;
    });

    mainWindow.loadFile('index.html');
  }

  // Window Controls IPC
  const handleWindowMinimize = () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.minimize();
    }
  };

  const handleWindowClose = () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.destroy();
    }
    app.quit();
  };

  ipcMain.handle('window:minimize', handleWindowMinimize);
  ipcMain.on('window:minimize', handleWindowMinimize);

  ipcMain.handle('window:close', handleWindowClose);
  ipcMain.on('window:close', handleWindowClose);

  // Fit window height to renderer content (width stays fixed)
  const WIN_WIDTH = 800;
  const MIN_HEIGHT = 400;
  ipcMain.on('window:fitContent', (event, height) => {
    if (!mainWindow || mainWindow.isDestroyed()) return;
    const h = Number(height);
    if (!Number.isFinite(h) || h <= 0) return;
    const workArea = screen.getDisplayMatching(mainWindow.getBounds()).workArea;
    const maxHeight = Math.max(MIN_HEIGHT, workArea.height - 40);
    const target = Math.round(Math.min(Math.max(h, MIN_HEIGHT), maxHeight));
    const [, currentH] = mainWindow.getContentSize();
    if (Math.abs(currentH - target) < 2) return;
    mainWindow.setResizable(true);
    mainWindow.setContentSize(WIN_WIDTH, target);
    mainWindow.setResizable(false);
    // Keep window fully on screen after growing
    const b = mainWindow.getBounds();
    const bottom = workArea.y + workArea.height;
    if (b.y + b.height > bottom) {
      mainWindow.setPosition(b.x, Math.max(workArea.y, bottom - b.height));
    }
  });

  const isValidSteamId3 = (id) => typeof id === 'string' && /^\d{1,16}$/.test(id);
  const isValidBackupFolder = (name) => typeof name === 'string' && /^[a-zA-Z0-9_\-\.]+$/.test(name) && !name.includes('..');

  // IPC Handlers
  ipcMain.handle('steam:getSystemStatus', async () => {
    const steam = steamService.detectSteam();
    const dota = steam.steamPath ? steamService.detectDota(steam.steamPath) : { installed: false, path: null };
    const isRunning = steamService.isSteamRunning();

    return {
      steam,
      dota,
      isSteamRunning: isRunning
    };
  });

  ipcMain.handle('steam:getAccounts', async () => {
    const steam = steamService.detectSteam();
    if (!steam.found || !steam.steamPath) {
      return [];
    }
    return steamService.getAccounts(steam.steamPath, steam.currentAutoLoginUser);
  });

  ipcMain.handle('steam:switchAccount', async (event, options) => {
    const steam = steamService.detectSteam();
    if (!steam.found || !steam.steamPath || !steam.steamExe) {
      throw new Error('Steam not detected on system.');
    }
    if (options && options.targetSteamId3 && !isValidSteamId3(options.targetSteamId3)) {
      throw new Error('Invalid Target Steam ID format.');
    }
    if (options && options.sourceSteamId3 && !isValidSteamId3(options.sourceSteamId3)) {
      throw new Error('Invalid Source Steam ID format.');
    }
    return await steamService.switchAccount(steam.steamPath, steam.steamExe, options);
  });

  ipcMain.handle('steam:syncDotaSettings', async (event, { sourceSteamId3, targetSteamId3, targetAccountName }) => {
    if (!isValidSteamId3(sourceSteamId3) || !isValidSteamId3(targetSteamId3)) {
      throw new Error('Invalid Steam ID format.');
    }
    const steam = steamService.detectSteam();
    if (!steam.found || !steam.steamPath) {
      throw new Error('Steam not detected on system.');
    }
    return await steamService.syncDotaSettings(steam.steamPath, sourceSteamId3, targetSteamId3, targetAccountName);
  });

  ipcMain.handle('steam:syncToAll', async (event, { sourceSteamId3 }) => {
    if (!isValidSteamId3(sourceSteamId3)) {
      throw new Error('Invalid Source Steam ID format.');
    }
    const steam = steamService.detectSteam();
    if (!steam.found || !steam.steamPath) {
      throw new Error('Steam not detected on system.');
    }

    const accounts = steamService.getAccounts(steam.steamPath, steam.currentAutoLoginUser);
    const results = [];

    for (const acc of accounts) {
      if (acc.steamId3 !== sourceSteamId3) {
        try {
          const res = await steamService.syncDotaSettings(steam.steamPath, sourceSteamId3, acc.steamId3, acc.accountName);
          results.push({ accountName: acc.accountName, personaName: acc.personaName, success: true, backup: res.backupCreated });
        } catch (err) {
          results.push({ accountName: acc.accountName, personaName: acc.personaName, success: false, error: err.message });
        }
      }
    }

    return { sourceSteamId3, syncedCount: results.filter(r => r.success).length, details: results };
  });

  ipcMain.handle('steam:listBackups', async () => {
    return steamService.listBackups();
  });

  ipcMain.handle('steam:restoreBackup', async (event, { backupFolderName, targetSteamId3 }) => {
    if (!isValidBackupFolder(backupFolderName)) {
      throw new Error('Invalid backup folder name format.');
    }
    if (!isValidSteamId3(targetSteamId3)) {
      throw new Error('Invalid Target Steam ID format.');
    }
    const steam = steamService.detectSteam();
    if (!steam.found || !steam.steamPath) {
      throw new Error('Steam not detected on system.');
    }
    return await steamService.restoreBackup(backupFolderName, targetSteamId3, steam.steamPath);
  });

  ipcMain.handle('system:openFolder', async (event, targetPath) => {
    try {
      const fullPath = targetPath ? path.normalize(targetPath) : steamService.getBackupsDir();
      if (fs.existsSync(fullPath)) {
        const stat = fs.statSync(fullPath);
        if (stat.isDirectory()) {
          const errMsg = await shell.openPath(fullPath);
          if (errMsg) {
            return { success: false, error: errMsg };
          }
          return { success: true };
        }
      }
      return { success: false, error: 'Target directory does not exist or is not a folder.' };
    } catch (err) {
      return { success: false, error: err.message };
    }
  });

  ipcMain.handle('system:openExternalUrl', async (event, url) => {
    if (url && (url.startsWith('https://') || url.startsWith('http://') || url.startsWith('steam://'))) {
      await shell.openExternal(url);
      return true;
    }
    return false;
  });

  ipcMain.handle('system:openSteamUrl', async (event, url) => {
    if (url && (url.startsWith('steam://') || url.startsWith('https://'))) {
      await shell.openExternal(url);
      return true;
    }
    return false;
  });

  ipcMain.handle('config:getSettings', async () => {
    return loadConfig();
  });

  ipcMain.handle('config:saveSettings', async (event, data) => {
    return saveConfig(data);
  });

  app.whenReady().then(() => {
    createWindow();

    app.on('activate', () => {
      if (BrowserWindow.getAllWindows().length === 0) createWindow();
    });
  });

  app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') app.quit();
  });
}
