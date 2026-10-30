ALTER TABLE categories ADD COLUMN featured_product_id TEXT REFERENCES products(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS product_media_product_order_idx ON product_media(product_id,sort_order,id);
