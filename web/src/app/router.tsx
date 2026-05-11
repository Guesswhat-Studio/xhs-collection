import { Navigate, createBrowserRouter } from 'react-router-dom';
import App from './App';
import { AlbumsPage } from '../pages/AlbumsPage';
import { LabelsPage } from '../pages/LabelsPage';
import { NotesPage } from '../pages/NotesPage';
import { StatsPage } from '../pages/StatsPage';

export const router = createBrowserRouter([
  {
    path: '/',
    element: <App />,
    children: [
      {
        index: true,
        element: <Navigate to="/notes" replace />,
      },
      {
        path: 'notes',
        element: <NotesPage />,
      },
      {
        path: 'albums',
        element: <AlbumsPage />,
      },
      {
        path: 'labels',
        element: <LabelsPage />,
      },
      {
        path: 'stats',
        element: <StatsPage />,
      },
    ],
  },
]);
