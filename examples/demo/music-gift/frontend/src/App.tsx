import { NavLink, Route, Routes } from 'react-router-dom';
import CreatePage from './pages/CreatePage';
import GiftPage from './pages/GiftPage';
import PlaylistPage from './pages/PlaylistPage';

export default function App() {
  return (
    <>
      <header className="app-header">
        <NavLink to="/" className="app-logo">
          Mo<span>ment</span>
        </NavLink>
        <nav className="app-nav">
          <NavLink to="/" end className={({ isActive }) => (isActive ? 'active' : '')}>
            Create
          </NavLink>
          <NavLink to="/playlist" className={({ isActive }) => (isActive ? 'active' : '')}>
            Playlist
          </NavLink>
        </nav>
      </header>
      <main className="app-main">
        <Routes>
          <Route path="/" element={<CreatePage />} />
          <Route path="/gift/:id" element={<GiftPage />} />
          <Route path="/playlist" element={<PlaylistPage />} />
        </Routes>
      </main>
    </>
  );
}
