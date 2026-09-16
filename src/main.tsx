import React from 'react';
import { createRoot } from 'react-dom/client';
import { AppTheme } from './AppTheme.tsx';
import '@radix-ui/themes/styles.css';
import 'react-day-picker/style.css';
import 'bootstrap-icons/font/bootstrap-icons.css';
import './styles.css';
import './i18n/index.ts';
import { App } from './App.tsx';
import { api } from './api.ts';

async function start() {
  // Apply a pending compatibility reset before React reads or persists theme preferences.
  try {
    if (await api.preferencesNeedReset()) {
      localStorage.removeItem('demodesk.appearance');
      localStorage.removeItem('demodesk.fontSize');
      await api.acknowledgePreferencesReset();
    }
  } catch (error) {
    // Do not mount preference writers when cleanup has not been acknowledged.
    const root = document.getElementById('root')!;
    root.textContent = String(error);
    return;
  }
  createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <AppTheme>
        <App />
      </AppTheme>
    </React.StrictMode>,
  );
}
void start();
