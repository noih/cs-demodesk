import React from 'react';
import { createRoot } from 'react-dom/client';
import { AppTheme } from './AppTheme.tsx';
import '@radix-ui/themes/styles.css';
import 'react-day-picker/style.css';
import 'bootstrap-icons/font/bootstrap-icons.css';
import './styles.css';
import './i18n/index.ts';
import { App } from './App.tsx';

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AppTheme>
      <App />
    </AppTheme>
  </React.StrictMode>,
);
