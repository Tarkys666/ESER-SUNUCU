PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS categories (
  id TEXT PRIMARY KEY CHECK (id GLOB '[a-z0-9]*'), name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 150),
  sort_order INTEGER NOT NULL DEFAULT 0, version INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS products (
  id TEXT PRIMARY KEY, category_id TEXT NOT NULL REFERENCES categories(id) ON DELETE RESTRICT,
  slug TEXT NOT NULL UNIQUE, title TEXT NOT NULL CHECK(length(trim(title)) BETWEEN 1 AND 250),
  summary TEXT NOT NULL DEFAULT '', content TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','published')),
  source_url TEXT, sort_order INTEGER NOT NULL DEFAULT 0, version INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS products_category_status_order_idx ON products(category_id,status,sort_order,id);
CREATE INDEX IF NOT EXISTS products_status_order_idx ON products(status,sort_order,id);
CREATE TABLE IF NOT EXISTS product_specs (
  id TEXT PRIMARY KEY, product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  label TEXT NOT NULL, value TEXT NOT NULL, unit TEXT NOT NULL DEFAULT '', sort_order INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS product_media (
  id TEXT PRIMARY KEY, product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  object_key TEXT NOT NULL UNIQUE, alt_text TEXT NOT NULL DEFAULT '', sort_order INTEGER NOT NULL DEFAULT 0,
  byte_size INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS hero_slides (
  id TEXT PRIMARY KEY, object_key TEXT NOT NULL UNIQUE, alt_text TEXT NOT NULL DEFAULT '',
  is_active INTEGER NOT NULL DEFAULT 1, sort_order INTEGER NOT NULL DEFAULT 0, version INTEGER NOT NULL DEFAULT 1,
  byte_size INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS pending_media_deletes (object_key TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS staged_media (object_key TEXT PRIMARY KEY, byte_size INTEGER NOT NULL, content_type TEXT NOT NULL);
