CREATE TABLE IF NOT EXISTS legal_profile (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  legal_name TEXT NOT NULL DEFAULT '',
  business_address TEXT NOT NULL DEFAULT '',
  privacy_email TEXT NOT NULL DEFAULT '',
  privacy_phone TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO legal_profile(id) VALUES (1);
