const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('switcheroo', {
  getSystemStatus: () => ipcRenderer.invoke('steam:getSystemStatus'),
  getAccounts: () => ipcRenderer.invoke('steam:getAccounts'),
  switchAccount: (options) => ipcRenderer.invoke('steam:switchAccount', options),
  syncDotaSettings: (options) => ipcRenderer.invoke('steam:syncDotaSettings', options),
  syncToAll: (options) => ipcRenderer.invoke('steam:syncToAll', options),
  listBackups: () => ipcRenderer.invoke('steam:listBackups'),
  restoreBackup: (options) => ipcRenderer.invoke('steam:restoreBackup', options),
  openFolder: (folderPath) => ipcRenderer.invoke('system:openFolder', folderPath),
  getSettings: () => ipcRenderer.invoke('config:getSettings'),
  saveSettings: (data) => ipcRenderer.invoke('config:saveSettings', data),
  openSteamUrl: (url) => ipcRenderer.invoke('system:openSteamUrl', url),
  openExternalUrl: (url) => ipcRenderer.invoke('system:openExternalUrl', url),
  minimizeWindow: () => {
    try { ipcRenderer.send('window:minimize'); } catch (_) {}
    return ipcRenderer.invoke('window:minimize');
  },
  closeWindow: () => {
    try { ipcRenderer.send('window:close'); } catch (_) {}
    return ipcRenderer.invoke('window:close');
  }
});
