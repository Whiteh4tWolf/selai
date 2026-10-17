import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import './styles.css';

// Apply the display preference before mounting the vault screen. Workbench
// applies later theme changes after unlock, but is not mounted while locked.
document.documentElement.dataset.theme = 'dark';
try {
  const theme = window.localStorage.getItem('selai.theme');
  if (theme === 'system') delete document.documentElement.dataset.theme;
  else if (theme === 'light') document.documentElement.dataset.theme = 'light';
} catch {
  // Keep the default dark surface when browser storage is unavailable.
}

const root = document.getElementById('root');
if (!root) {
  throw new Error('missing #root element');
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
