import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './app/App';
import '@fontsource-variable/noto-sans-sc/index.css';
import '@fontsource-variable/noto-serif-sc/index.css';
import 'lxgw-wenkai-screen-webfont/lxgwwenkaigbscreen.css';
import './styles/global.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
