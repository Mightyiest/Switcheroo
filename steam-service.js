const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { execSync, spawn, spawnSync } = require('child_process');

function atomicWriteFileSync(filePath, content, encoding = 'utf8') {
  const tmpPath = `${filePath}.${Date.now()}-${Math.random().toString(36).slice(2)}.tmp`;
  fs.writeFileSync(tmpPath, content, encoding);
  fs.renameSync(tmpPath, filePath);
}

/**
 * Settings files (relative to userdata/<id>/570) that are safe to share between accounts.
 * Deliberately excludes remotecache.vdf (Steam Cloud's per-account sync state) and
 * account-specific data such as stats.dat, last_match.dat, voice_ban.dt, favorite_friends.json.
 */
const SHARED_REMOTE_FILES = [
  'remote/cfg/dotakeys_personal.lst',      // main hotkeys, quickcast, item keys
  'remote/cfg/herobuilds.cfg',
  'remote/cfg/hero_facet_config.cfg',
  'remote/user_keys.vcfg',                 // Source 2 console binds
  'remote/user_convars.vcfg',              // game options, chat wheel, camera, etc.
  'remote/scripts/control_groups.txt',
  'remote/scripts/item_suggest_preference.txt'
];
// Local (non-cloud) settings: active key/convar slots + video settings
const SHARED_LOCAL_PATTERN = /^(user_keys_\d+_slot\d+\.vcfg|user_convars_\d+_slot\d+\.vcfg)(_lastclouded)?$|^(machine_convars\.vcfg|video\.txt)$/i;
// Files used to decide whether an account's hotkeys match the master
const HOTKEY_FILES = ['remote/cfg/dotakeys_personal.lst', 'remote/user_keys.vcfg'];

function sha1File(file) {
  return crypto.createHash('sha1').update(fs.readFileSync(file)).digest('hex');
}

function getHotkeysHash(dotaDir) {
  const h = crypto.createHash('sha1');
  let found = false;
  for (const rel of HOTKEY_FILES) {
    const f = path.join(dotaDir, rel);
    if (fs.existsSync(f)) {
      h.update(fs.readFileSync(f));
      found = true;
    }
  }
  return found ? h.digest('hex') : null;
}

// Standard Steam base ID for converting 64-bit ID to 32-bit AccountID
const STEAM_ID_BASE = 76561197960265728n;

/**
 * Executes a shell command synchronously and returns stdout
 */
function runCommand(cmd) {
  try {
    return execSync(cmd, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'] }).trim();
  } catch (err) {
    return null;
  }
}

/**
 * Automatically detects Steam installation path via Windows Registry or common directories
 */
function detectSteam() {
  let steamPath = null;
  let steamExe = null;
  let currentAutoLogin = null;

  // 1. Try Windows Registry: HKCU\Software\Valve\Steam
  try {
    const regOutput = runCommand('reg query "HKCU\\Software\\Valve\\Steam"');
    if (regOutput) {
      const pathMatch = regOutput.match(/SteamPath\s+REG_SZ\s+(.*)/i);
      if (pathMatch && pathMatch[1]) {
        steamPath = path.normalize(pathMatch[1].trim());
      }

      const exeMatch = regOutput.match(/SteamExe\s+REG_SZ\s+(.*)/i);
      if (exeMatch && exeMatch[1]) {
        steamExe = path.normalize(exeMatch[1].trim());
      }

      const autoMatch = regOutput.match(/AutoLoginUser\s+REG_SZ\s+(.*)/i);
      if (autoMatch && autoMatch[1]) {
        currentAutoLogin = autoMatch[1].trim();
      }
    }
  } catch (err) {
    // Registry query failed, proceed to fallback
  }

  // 2. Fallbacks if registry didn't return a valid path
  if (!steamPath || !fs.existsSync(steamPath)) {
    const commonPaths = [
      'C:\\Program Files (x86)\\Steam',
      'C:\\Program Files\\Steam',
      'D:\\Steam',
      'D:\\Application\\Steam',
      'E:\\Steam',
      'E:\\Application\\Steam'
    ];
    for (const testPath of commonPaths) {
      if (fs.existsSync(testPath) && fs.existsSync(path.join(testPath, 'steam.exe'))) {
        steamPath = testPath;
        steamExe = path.join(testPath, 'steam.exe');
        break;
      }
    }
  }

  if (steamPath && !steamExe) {
    const defaultExe = path.join(steamPath, 'steam.exe');
    if (fs.existsSync(defaultExe)) {
      steamExe = defaultExe;
    }
  }

  const isValid = steamPath && fs.existsSync(steamPath) && steamExe && fs.existsSync(steamExe);

  return {
    found: Boolean(isValid),
    steamPath: steamPath ? path.normalize(steamPath) : null,
    steamExe: steamExe ? path.normalize(steamExe) : null,
    currentAutoLoginUser: currentAutoLogin || null
  };
}

/**
 * Automatically detects Dota 2 installation (checks steamapps and libraryfolders.vdf)
 */
function detectDota(steamPath) {
  if (!steamPath || !fs.existsSync(steamPath)) {
    return { installed: false, path: null };
  }

  // Check 1: Direct in Steam folder
  const standardExe = path.join(steamPath, 'steamapps', 'common', 'dota 2 beta', 'game', 'bin', 'win64', 'dota2.exe');
  if (fs.existsSync(standardExe)) {
    return {
      installed: true,
      path: standardExe,
      directory: path.join(steamPath, 'steamapps', 'common', 'dota 2 beta')
    };
  }

  // Check 2: libraryfolders.vdf
  const libraryFoldersPath = path.join(steamPath, 'config', 'libraryfolders.vdf');
  if (fs.existsSync(libraryFoldersPath)) {
    try {
      const content = fs.readFileSync(libraryFoldersPath, 'utf8');
      const pathRegex = /"path"\s+"([^"]+)"/g;
      let match;
      while ((match = pathRegex.exec(content)) !== null) {
        let libPath = match[1].replace(/\\\\/g, '\\');
        const testDotaExe = path.join(libPath, 'steamapps', 'common', 'dota 2 beta', 'game', 'bin', 'win64', 'dota2.exe');
        if (fs.existsSync(testDotaExe)) {
          return {
            installed: true,
            path: testDotaExe,
            directory: path.join(libPath, 'steamapps', 'common', 'dota 2 beta')
          };
        }
      }
    } catch (err) {
      console.error('Error parsing libraryfolders.vdf:', err);
    }
  }

  return { installed: false, path: null };
}

/**
 * Reads Dota 2 LaunchOptions for a specific SteamID3 from localconfig.vdf
 */
function getDotaLaunchOptions(steamPath, steamId3) {
  if (!steamPath || !steamId3) return '';
  const file = path.join(steamPath, 'userdata', steamId3, 'config', 'localconfig.vdf');
  if (!fs.existsSync(file)) return '';
  try {
    const raw = fs.readFileSync(file, 'utf8');
    const match = raw.match(/"570"[\s\S]*?"LaunchOptions"\s+"([^"]*)"/);
    if (match) return match[1];
  } catch (err) {
    console.error('Error reading launch options:', err);
  }
  return '';
}

/**
 * Sets Dota 2 LaunchOptions in localconfig.vdf for a specific SteamID3
 */
function setDotaLaunchOptions(steamPath, steamId3, launchOptions) {
  if (!steamPath || !steamId3) return false;
  const file = path.join(steamPath, 'userdata', steamId3, 'config', 'localconfig.vdf');
  if (!fs.existsSync(file)) return false;
  try {
    let raw = fs.readFileSync(file, 'utf8');
    const appIndex = raw.search(/"570"\s*\{/i);
    if (appIndex !== -1) {
      const slice = raw.slice(appIndex, appIndex + 2500);
      if (/"LaunchOptions"\s+"[^"]*"/i.test(slice)) {
        raw = raw.replace(/"570"([\s\S]*?)"LaunchOptions"\s+"[^"]*"/, `"570"$1"LaunchOptions"\t\t"${launchOptions || ''}"`);
      } else {
        raw = raw.replace(/"570"\s*\{/i, `"570"\n\t\t\t\t\t{\n\t\t\t\t\t\t"LaunchOptions"\t\t"${launchOptions || ''}"`);
      }
      fs.writeFileSync(file, raw, 'utf8');
      return true;
    }
  } catch (err) {
    console.error('Failed to update localconfig LaunchOptions:', err);
  }
  return false;
}

/**
 * Parses loginusers.vdf and retrieves all accounts with their avatars and Dota 2 status
 */
function getAccounts(steamPath, currentAutoLoginUser) {
  if (!steamPath) return [];

  const loginUsersPath = path.join(steamPath, 'config', 'loginusers.vdf');
  if (!fs.existsSync(loginUsersPath)) return [];

  const raw = fs.readFileSync(loginUsersPath, 'utf8');
  const lines = raw.split(/\r?\n/);
  const accounts = [];

  let currentSteamId = null;
  let currentUser = null;

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.toLowerCase() === '"users"' || trimmed === '{') continue;

    const idMatch = trimmed.match(/^"(\d{15,25})"/);
    if (idMatch && !currentUser) {
      currentSteamId = idMatch[1];
      currentUser = { steamId64: currentSteamId };
      continue;
    }

    if (trimmed === '}') {
      if (currentUser && currentUser.AccountName) {
        accounts.push(currentUser);
        currentUser = null;
        currentSteamId = null;
      }
      continue;
    }

    if (currentUser) {
      const kvMatch = trimmed.match(/^"([^"]+)"\s+"(.*)"$/);
      if (kvMatch) {
        currentUser[kvMatch[1]] = kvMatch[2];
      }
    }
  }

  const avatarCacheDir = path.join(steamPath, 'config', 'avatarcache');
  const userdataDir = path.join(steamPath, 'userdata');

  return accounts.map(acc => {
    let steamId3 = '';
    try {
      steamId3 = (BigInt(acc.steamId64) - STEAM_ID_BASE).toString();
    } catch (e) {
      steamId3 = acc.steamId64;
    }

    // Avatar retrieval
    let avatarBase64 = null;
    const avatarFile = path.join(avatarCacheDir, `${acc.steamId64}.png`);
    if (fs.existsSync(avatarFile)) {
      try {
        const imgBuffer = fs.readFileSync(avatarFile);
        avatarBase64 = `data:image/png;base64,${imgBuffer.toString('base64')}`;
      } catch (err) {
        console.error(`Failed to load avatar for ${acc.steamId64}:`, err);
      }
    }

    // Check Dota 2 config status (App ID 570)
    const dotaUserdataDir = path.join(userdataDir, steamId3, '570');
    const hasDotaConfig = fs.existsSync(dotaUserdataDir);
    let dotaLastUpdated = null;
    let dotaFilesCount = 0;
    let dotakeysSize = 0;
    const hotkeysHash = hasDotaConfig ? getHotkeysHash(dotaUserdataDir) : null;

    if (hasDotaConfig) {
      try {
        const dotakeysFile = path.join(dotaUserdataDir, 'remote', 'cfg', 'dotakeys_personal.lst');
        if (fs.existsSync(dotakeysFile)) {
          const stat = fs.statSync(dotakeysFile);
          dotaLastUpdated = stat.mtime.toISOString();
          dotakeysSize = stat.size;
        } else {
          const stat = fs.statSync(dotaUserdataDir);
          dotaLastUpdated = stat.mtime.toISOString();
        }

        const countFiles = (dir) => {
          let count = 0;
          if (!fs.existsSync(dir)) return 0;
          const items = fs.readdirSync(dir, { withFileTypes: true });
          for (const item of items) {
            if (item.isDirectory()) {
              count += countFiles(path.join(dir, item.name));
            } else {
              count++;
            }
          }
          return count;
        };
        dotaFilesCount = countFiles(dotaUserdataDir);
      } catch (e) {
        // Ignored
      }
    }

    // launchOptions are managed by application settings rather than heavy synchronous localconfig parsing
    const launchOptions = '';

    const isActive = Boolean(
      (currentAutoLoginUser && acc.AccountName && acc.AccountName.toLowerCase() === currentAutoLoginUser.toLowerCase()) ||
      acc.AutoLogin === '1'
    );

    return {
      steamId64: acc.steamId64,
      steamId3: steamId3,
      accountName: acc.AccountName || '',
      personaName: acc.PersonaName || acc.AccountName || 'Steam User',
      rememberPassword: acc.RememberPassword === '1',
      isActive: isActive,
      avatar: avatarBase64,
      hasDotaConfig: hasDotaConfig,
      hotkeysHash: hotkeysHash,
      dotaLastUpdated: dotaLastUpdated,
      dotaFilesCount: dotaFilesCount,
      dotakeysSize: dotakeysSize,
      launchOptions: launchOptions,
      timestamp: acc.Timestamp || '0'
    };
  });
}

/**
 * Checks if Steam is currently running
 */
function isSteamRunning() {
  const output = runCommand('tasklist /FI "IMAGENAME eq steam.exe" /NH');
  return output && output.toLowerCase().includes('steam.exe');
}

/**
 * Gracefully terminates Steam process
 */
async function killSteam() {
  if (!isSteamRunning()) return true;

  try {
    execSync('taskkill /IM steam.exe', { stdio: 'ignore' });
  } catch (e) {
    // Ignore error
  }

  for (let i = 0; i < 6; i++) {
    await new Promise(r => setTimeout(r, 400));
    if (!isSteamRunning()) return true;
  }

  try {
    execSync('taskkill /F /IM steam.exe', { stdio: 'ignore' });
  } catch (e) {
    // Ignore error
  }

  await new Promise(r => setTimeout(r, 500));
  return true;
}

/**
 * Copies directory recursively and updates modification timestamps
 */
function copyDirSync(src, dest) {
  if (!fs.existsSync(dest)) {
    fs.mkdirSync(dest, { recursive: true });
  }
  const entries = fs.readdirSync(src, { withFileTypes: true });

  for (const entry of entries) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);

    if (entry.isDirectory()) {
      copyDirSync(srcPath, destPath);
    } else {
      fs.copyFileSync(srcPath, destPath);
      const now = new Date();
      try {
        fs.utimesSync(destPath, now, now);
      } catch (e) {
        // Ignore
      }
    }
  }
}

/**
 * Gets the Switcheroo backups folder
 */
function getBackupsDir() {
  const baseDir = process.env.PORTABLE_EXECUTABLE_DIR || __dirname;
  const dir = path.join(baseDir, 'backups');
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  return dir;
}

/**
 * Returns true if Dota 2 is currently running
 */
function isDotaRunning() {
  const output = runCommand('tasklist /FI "IMAGENAME eq dota2.exe" /NH');
  return Boolean(output && output.toLowerCase().includes('dota2.exe'));
}

/**
 * Builds the list of [relativePath] settings files to copy from a source 570 dir
 */
function listSharedFiles(sourceDotaDir) {
  const files = SHARED_REMOTE_FILES.filter(rel => fs.existsSync(path.join(sourceDotaDir, rel)));
  const localCfg = path.join(sourceDotaDir, 'local', 'cfg');
  if (fs.existsSync(localCfg)) {
    for (const name of fs.readdirSync(localCfg)) {
      if (SHARED_LOCAL_PATTERN.test(name)) files.push(`local/cfg/${name}`);
    }
  }
  return files;
}

/**
 * Synchronizes Dota 2 settings from sourceSteamId3 to targetSteamId3.
 *
 * IMPORTANT: the target's own remotecache.vdf is left untouched. Steam then sees the
 * settings files as "changed locally" against an unchanged cloud and UPLOADS them.
 * (Copying the master's remotecache made Steam think the cloud was newer and it
 * silently re-downloaded the alt's old hotkeys.)
 */
async function syncDotaSettings(steamPath, sourceSteamId3, targetSteamId3, targetAccountName) {
  if (!steamPath) throw new Error('Steam path is required');
  if (sourceSteamId3 === targetSteamId3) {
    return { success: true, skipped: true, reason: 'Source and target accounts are identical.' };
  }

  // Dota writes its in-memory settings on exit, which would overwrite our copy
  if (isDotaRunning()) {
    throw new Error('Dota 2 is running. Close Dota 2 first, then sync.');
  }

  // Ensure Steam is stopped before copying files so Steam Cloud can't interfere mid-copy
  await killSteam();

  const userdataDir = path.join(steamPath, 'userdata');
  const sourceDotaDir = path.join(userdataDir, sourceSteamId3, '570');
  const targetDotaDir = path.join(userdataDir, targetSteamId3, '570');

  if (!fs.existsSync(sourceDotaDir)) {
    throw new Error(`Master account does not have Dota 2 settings in: ${sourceDotaDir}`);
  }

  // 1. Create a timestamped backup of the target's current settings if they exist
  let backupCreated = null;
  if (fs.existsSync(targetDotaDir)) {
    const backupsDir = getBackupsDir();
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const backupFolderName = `${targetAccountName || targetSteamId3}_backup_${timestamp}`;
    const backupPath = path.join(backupsDir, backupFolderName);
    copyDirSync(targetDotaDir, backupPath);
    backupCreated = {
      folderName: backupFolderName,
      path: backupPath,
      timestamp: new Date().toISOString()
    };
  }

  // 2. Copy only the allowlisted settings files, stamping them as freshly modified
  const files = listSharedFiles(sourceDotaDir);
  const now = new Date();
  for (const rel of files) {
    const src = path.join(sourceDotaDir, rel);
    const dst = path.join(targetDotaDir, rel);
    fs.mkdirSync(path.dirname(dst), { recursive: true });
    fs.copyFileSync(src, dst);
    try { fs.utimesSync(dst, now, now); } catch (e) { /* ignore */ }
  }

  // 3. Verify every copied file is byte-identical to the master
  const mismatched = files.filter(rel =>
    sha1File(path.join(sourceDotaDir, rel)) !== sha1File(path.join(targetDotaDir, rel))
  );
  if (mismatched.length > 0) {
    throw new Error(`Verification failed for: ${mismatched.join(', ')}`);
  }

  return {
    success: true,
    filesCopied: files,
    backupCreated: backupCreated
  };
}

/**
 * Switches the active Steam account, with optional automatic settings sync & launch options
 */
async function switchAccount(steamPath, steamExe, options) {
  const {
    targetAccountName,
    targetSteamId3,
    sourceSteamId3,
    autoSyncSettings = false,
    launchDota = false,
    launchOptions = ''
  } = options;

  if (!steamPath || !steamExe) {
    throw new Error('Steam path or executable not found.');
  }

  // If already logged in on this account and Steam is running, launch Dota 2 directly without restarting Steam
  const currentSteam = detectSteam();
  const isAlreadyActive = currentSteam.currentAutoLoginUser && 
    targetAccountName && 
    currentSteam.currentAutoLoginUser.toLowerCase() === targetAccountName.toLowerCase();

  if (isAlreadyActive && isSteamRunning()) {
    if (launchDota) {
      let args = ['-applaunch', '570'];
      if (launchOptions) {
        const parsedArgs = launchOptions.match(/(?:[^\s"]+|"[^"]*")+/g) || [];
        args.push(...parsedArgs.map(a => a.replace(/^"|"$/g, '')));
      }
      const subprocess = spawn(steamExe, args, { detached: true, stdio: 'ignore' });
      subprocess.unref();
      return { success: true, launchedDota: true, restartedSteam: false, alreadyActive: true };
    }
    return { success: true, alreadyActive: true, restartedSteam: false };
  }

  // 1. Ensure Steam is fully closed before switching accounts or syncing
  await killSteam();

  // 2. Auto-sync Dota 2 settings if enabled
  let syncResult = null;
  if (autoSyncSettings && sourceSteamId3 && targetSteamId3 && sourceSteamId3 !== targetSteamId3) {
    syncResult = await syncDotaSettings(steamPath, sourceSteamId3, targetSteamId3, targetAccountName);
  }

  // 3. Launch options are passed on the command line (-applaunch 570 <opts>) below.
  //    localconfig.vdf is intentionally NOT edited: Steam rewrites it while running,
  //    and regex-editing it risks touching another game's LaunchOptions.

  // 4. Update Windows Registry AutoLoginUser safely using spawnSync
  try {
    const regAuto = spawnSync('reg.exe', [
      'add', 'HKCU\\Software\\Valve\\Steam',
      '/v', 'AutoLoginUser',
      '/t', 'REG_SZ',
      '/d', targetAccountName,
      '/f'
    ], { windowsHide: true });
    if (regAuto.error) throw regAuto.error;

    const regRemember = spawnSync('reg.exe', [
      'add', 'HKCU\\Software\\Valve\\Steam',
      '/v', 'RememberPassword',
      '/t', 'REG_DWORD',
      '/d', '1',
      '/f'
    ], { windowsHide: true });
    if (regRemember.error) throw regRemember.error;
  } catch (err) {
    throw new Error(`Failed to update Windows Registry: ${err.message}`);
  }

  // 5. Update loginusers.vdf so AutoLogin is 1 for target and 0 for others
  const loginUsersPath = path.join(steamPath, 'config', 'loginusers.vdf');
  if (fs.existsSync(loginUsersPath)) {
    try {
      let content = fs.readFileSync(loginUsersPath, 'utf8');
      const lines = content.split(/\r?\n/);
      let currentAccount = null;
      const modifiedLines = [];

      for (let i = 0; i < lines.length; i++) {
        let line = lines[i];
        const trimmed = line.trim();

        const accountMatch = trimmed.match(/^"AccountName"\s+"([^"]+)"$/i);
        if (accountMatch) {
          currentAccount = accountMatch[1];
        }

        const autoLoginMatch = trimmed.match(/^"AutoLogin"\s+"([^"]+)"$/i);
        if (autoLoginMatch && currentAccount) {
          const isTarget = currentAccount.toLowerCase() === targetAccountName.toLowerCase();
          line = line.replace(/"AutoLogin"\s+"[^"]+"/, `"AutoLogin"\t\t"${isTarget ? '1' : '0'}"`);
        }

        if (trimmed === '}') {
          currentAccount = null;
        }

        modifiedLines.push(line);
      }

      atomicWriteFileSync(loginUsersPath, modifiedLines.join('\r\n'), 'utf8');
    } catch (err) {
      console.warn('Warning: Could not update loginusers.vdf directly:', err.message);
    }
  }

  // 6. Launch Steam (with Dota 2 launch options if requested)
  let args = [];
  if (launchDota) {
    args = ['-applaunch', '570'];
    if (launchOptions) {
      // Split options into tokens preserving quotes
      const parsedArgs = launchOptions.match(/(?:[^\s"]+|"[^"]*")+/g) || [];
      args.push(...parsedArgs.map(a => a.replace(/^"|"$/g, '')));
    }
  }

  const subprocess = spawn(steamExe, args, {
    detached: true,
    stdio: 'ignore'
  });
  subprocess.unref();

  return { success: true, launchedDota: launchDota, synced: Boolean(syncResult) };
}

/**
 * Lists available backups
 */
function listBackups() {
  const backupsDir = getBackupsDir();
  if (!fs.existsSync(backupsDir)) return [];

  const items = fs.readdirSync(backupsDir, { withFileTypes: true });
  const backups = [];

  for (const item of items) {
    if (item.isDirectory()) {
      const fullPath = path.join(backupsDir, item.name);
      const stat = fs.statSync(fullPath);
      backups.push({
        name: item.name,
        path: fullPath,
        created: stat.mtime.toISOString(),
        sizeFormatted: `${(stat.size / 1024).toFixed(1)} KB`
      });
    }
  }

  return backups.sort((a, b) => new Date(b.created) - new Date(a.created));
}

/**
 * Restores a backup to target account's 570 directory
 */
async function restoreBackup(backupFolderName, targetSteamId3, steamPath) {
  const backupsDir = getBackupsDir();
  const backupPath = path.join(backupsDir, backupFolderName);
  if (!fs.existsSync(backupPath)) {
    throw new Error('Backup folder not found');
  }

  if (isDotaRunning()) {
    throw new Error('Dota 2 is running. Close Dota 2 first, then restore.');
  }

  await killSteam();

  // Restore settings files only; never restore remotecache.vdf (stale cloud state)
  const targetDotaDir = path.join(steamPath, 'userdata', targetSteamId3, '570');
  const files = listSharedFiles(backupPath);
  const now = new Date();
  for (const rel of files) {
    const dst = path.join(targetDotaDir, rel);
    fs.mkdirSync(path.dirname(dst), { recursive: true });
    fs.copyFileSync(path.join(backupPath, rel), dst);
    try { fs.utimesSync(dst, now, now); } catch (e) { /* ignore */ }
  }
  return { success: true, filesRestored: files };
}

module.exports = {
  detectSteam,
  detectDota,
  getAccounts,
  getDotaLaunchOptions,
  setDotaLaunchOptions,
  switchAccount,
  syncDotaSettings,
  listBackups,
  restoreBackup,
  getBackupsDir,
  isSteamRunning,
  isDotaRunning,
  killSteam
};
