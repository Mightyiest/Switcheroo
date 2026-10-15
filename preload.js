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
  minimizeWindow: () => ipcRenderer.invoke('window:minimize'),
  maximizeWindow: () => ipcRenderer.invoke('window:maximize'),
  onMaximizedChange: (callback) => {
    const subscription = (event, isMax) => callback(isMax);
    ipcRenderer.on('window:isMaximized', subscription);
    return () => ipcRenderer.removeListener('window:isMaximized', subscription);
  }
});
