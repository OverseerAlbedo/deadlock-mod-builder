import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import { startLanguage } from './i18n/startup';
import './styles.css';
import './workspace.css';
import './i18n/lang.css';

// The language's catalog is loaded before the first render, so the first paint is already in it.
await startLanguage();
ReactDOM.createRoot(document.getElementById('root')!).render(<React.StrictMode><App /></React.StrictMode>);
