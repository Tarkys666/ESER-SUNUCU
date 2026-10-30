ALTER TABLE products ADD COLUMN trendyol_url TEXT CHECK (trendyol_url IS NULL OR (length(trendyol_url) BETWEEN 9 AND 2048 AND substr(trendyol_url, 1, 8) = 'https://'));
ALTER TABLE products ADD COLUMN idefix_url TEXT CHECK (idefix_url IS NULL OR (length(idefix_url) BETWEEN 9 AND 2048 AND substr(idefix_url, 1, 8) = 'https://'));
ALTER TABLE products ADD COLUMN hepsiburada_url TEXT CHECK (hepsiburada_url IS NULL OR (length(hepsiburada_url) BETWEEN 9 AND 2048 AND substr(hepsiburada_url, 1, 8) = 'https://'));
ALTER TABLE products ADD COLUMN pazarama_url TEXT CHECK (pazarama_url IS NULL OR (length(pazarama_url) BETWEEN 9 AND 2048 AND substr(pazarama_url, 1, 8) = 'https://'));
ALTER TABLE products ADD COLUMN n11_url TEXT CHECK (n11_url IS NULL OR (length(n11_url) BETWEEN 9 AND 2048 AND substr(n11_url, 1, 8) = 'https://'));
