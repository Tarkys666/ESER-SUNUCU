ALTER TABLE categories ADD COLUMN cover_path TEXT;

-- Initial covers for the existing showcase groups; product media stays unchanged.
UPDATE categories SET cover_path='assets/category-covers/equipment.jpg' WHERE id IN ('aa','cc');
UPDATE categories SET cover_path='assets/category-covers/threading.jpg' WHERE id='bb';
UPDATE categories SET cover_path='assets/category-covers/inspection.jpg' WHERE id='bbbbbbb';
