CREATE TABLE IF NOT EXISTS product_view_daily (
  product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  view_date TEXT NOT NULL,
  view_count INTEGER NOT NULL DEFAULT 0 CHECK (view_count >= 0),
  PRIMARY KEY (product_id, view_date)
);
