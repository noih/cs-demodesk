import React from 'react';
import { createRoot } from 'react-dom/client';
import { Theme } from '@radix-ui/themes';
import '@radix-ui/themes/styles.css';
import 'react-day-picker/style.css';
import './styles.css';
import './i18n/index.ts';
import { App } from './App.tsx';

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Theme appearance="dark" accentColor="amber" grayColor="slate" radius="medium" scaling="95%">
      <App />
    </Theme>
  </React.StrictMode>,
);
