-- Add the initial catalog groups without overwriting existing admin edits.
INSERT INTO categories (id, name, sort_order, cover_path) VALUES
('termal-kameralar', 'Termal Kameralar', 1, 'assets/catalogs/termal-kameralar.jpeg'),
('kanal-goruntuleme-kameralari', 'Kanal Görüntüleme Kameraları', 2, 'assets/catalogs/kanal-goruntuleme-kameralari.jpeg'),
('manuel-test-pompalari', 'Manuel Test Pompaları', 3, 'assets/catalogs/manuel-test-pompalari.jpeg'),
('flex-saftli-kanal-acma-makineleri', 'Flex Şaftlı Kanal Açma Makineleri', 4, 'assets/catalogs/flex-saftli-kanal-acma-makineleri.jpeg'),
('plastik-boru-kaynak-makineleri', 'Plastik Boru Kaynak Makineleri', 5, 'assets/catalogs/plastik-boru-kaynak-makineleri.jpeg'),
('akulu-budama-makaslari', 'Akülü Budama Makasları', 6, 'assets/catalogs/akulu-budama-makaslari.jpeg'),
('petek-temizleme-makineleri', 'Petek Temizleme Makineleri', 7, 'assets/catalogs/petek-temizleme-makineleri.jpeg'),
('otomatik-test-pompalari', 'Otomatik Test Pompaları', 8, 'assets/catalogs/otomatik-test-pompalari.jpeg'),
('basincli-yikama-makineleri', 'Basınçlı Yıkama Makineleri', 9, 'assets/catalogs/basincli-yikama-makineleri.jpeg'),
('karot-makineleri', 'Karot Makineleri', 10, 'assets/catalogs/karot-makineleri.jpeg'),
('kanal-acma-yaylari-ve-uclari', 'Kanal Açma Yayları ve Uçları', 11, 'assets/catalogs/kanal-acma-yaylari-ve-uclari.jpeg'),
('su-kacak-tespit-cihazlari', 'Su Kaçak Tespit Cihazları', 12, 'assets/catalogs/su-kacak-tespit-cihazlari.jpeg'),
('boru-dis-acma-makineleri', 'Boru Diş Açma Makineleri', 13, 'assets/catalogs/boru-dis-acma-makineleri.jpeg'),
('spiralli-kanal-acma-makineleri', 'Spiralli Kanal Açma Makineleri', 14, 'assets/catalogs/spiralli-kanal-acma-makineleri.jpeg'),
('boru-kesme-makaslari', 'Boru Kesme Makasları', 15, 'assets/catalogs/boru-kesme-makaslari.jpeg')
ON CONFLICT(id) DO NOTHING;
