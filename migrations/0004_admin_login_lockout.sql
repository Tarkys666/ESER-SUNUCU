CREATE TABLE IF NOT EXISTS admin_login_locks (
  ip_key TEXT PRIMARY KEY,
  locked_until INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS admin_login_locks_expiry_idx ON admin_login_locks(locked_until);
