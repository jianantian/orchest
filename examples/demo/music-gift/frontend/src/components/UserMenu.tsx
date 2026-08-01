import { useEffect, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import type { User } from '../hooks/useAuth';
import { useI18n } from '../i18n';

/**
 * Avatar button + account menu: shows the signed-in user's identity and
 * account actions (set password, sign out). Sign-out is an explicit menu
 * item — clicking the avatar itself must never log the user out.
 */
export function UserMenu({ user, onLogout }: { user: User; onLogout: () => void }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Close on outside click or Escape.
  useEffect(() => {
    if (!open) return;
    function onDocClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') setOpen(false);
    }
    document.addEventListener('mousedown', onDocClick);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDocClick);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <div className="user-menu" ref={ref}>
      <button
        className="user-btn"
        aria-expanded={open}
        aria-label={user.display_name}
        title={user.display_name}
        onClick={() => setOpen((o) => !o)}
      >
        {user.display_name[0].toUpperCase()}
      </button>
      {open && (
        <div className="user-menu-drop" role="menu">
          <div className="user-menu-head">
            <span className="user-menu-name">{user.display_name}</span>
            {user.email && <span className="user-menu-email">{user.email}</span>}
          </div>
          <Link
            to="/set-password"
            className="user-menu-item"
            role="menuitem"
            onClick={() => setOpen(false)}
          >
            {t('setpw_title')}
          </Link>
          <button
            className="user-menu-item user-menu-logout"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              void onLogout();
            }}
          >
            {t('menu_logout')}
          </button>
        </div>
      )}
    </div>
  );
}
