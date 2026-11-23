import { app, BrowserWindow, Menu, session } from 'electron';
import * as path from 'node:path';

function createWindow(): void {
  const win = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 800,
    minHeight: 500,
    title: 'Car Game',
    backgroundColor: '#0b0f17',
    autoHideMenuBar: true,
    // Window and taskbar icon on Linux; macOS and Windows use the app's own icon.
    icon: path.join(__dirname, '..', 'build', 'icon.png'),
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  // The game never opens other pages or windows.
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  win.webContents.on('will-navigate', (event) => event.preventDefault());

  void win.loadFile(path.join(__dirname, '..', 'dist', 'index.html'));
}

app.whenReady().then(() => {
  // macOS keeps its standard menu (Cmd+Q, copy/paste). Other systems get none.
  if (process.platform !== 'darwin') Menu.setApplicationMenu(null);
  createWindow();
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

// Write saved settings (localStorage) to disk before the app exits.
app.on('before-quit', () => session.defaultSession.flushStorageData());

app.on('window-all-closed', () => app.quit());
