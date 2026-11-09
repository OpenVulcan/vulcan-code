import type { BrowserWindow, NavigationHistory } from 'electron';

/** The window capabilities needed to leave the local startup surface. */
type WorkbenchPageWindow = Pick<BrowserWindow, 'loadURL'> & {
  webContents: { navigationHistory: Pick<NavigationHistory, 'clear'> };
};

/** Loads the trusted workbench without changing history before navigation succeeds. */
export async function loadWorkbenchPage(window: WorkbenchPageWindow, webUiUrl: string): Promise<void> {
  await window.loadURL(webUiUrl);
  // The startup surface is a one-shot status display, not a history destination to reconnect Core.
  window.webContents.navigationHistory.clear();
}
