# Dilek Makina Cloudflare backend

30 Ekim 2026. Yerel uygulama tamamlandı; kullanıcı isteğiyle ana depolara gönderiliyor.

## Mimari

- `ESER WEB`: mevcut statik frontend, Cloudflare Pages `eser-web.pages.dev` olarak dağıtılıyor.
- `ESER SUNUCU`: Rust/Wasm Worker, D1 katalog verisi, özel R2 medya bucket'ı.
- Supabase bu uygulama akışından çıkarılıyor. Admin e-posta ve parola Cloudflare Worker Secrets'tan okunur; oturum imza anahtarı ayrı secret olmalıdır.
- Yerelde web `127.0.0.1:4173`, Worker `127.0.0.1:8787` adreslerinde çalışır. Worker `.dev.vars` Git'e eklenmez.
- Kullanıcıdan parola sohbet içinde istenmez. Kullanıcı Cloudflare Secrets Store'a yönetici bilgilerini kendisi girecek.

## Uygulama ilkeleri

- Tek Worker API istemcisi ve parametreli SQL kullan; Supabase istemcisini taklit eden genel adapter/ORM ekleme.
- Public katalog yalnız yayındaki ürünleri, aktif slider'ı ve bağlı görselleri gösterir. Taslak medyası public değildir.
- Admin her yazma isteğinde oturum doğrular. Görsel türü ve 5 MB sınırı sunucuda da denetlenir; R2 nesne anahtarları sunucuda üretilir.
- Kategori silme, bağlı ürün varsa D1 foreign key ile reddedilir. Ürün ve alt kayıtlar beraber yönetilir.
- Kullanım paneli içerik sayaçlarını gösterir; bu sayaçlar Cloudflare hesap kotası/faturası diye sunulmaz.
- Var olan kullanıcı değişiklikleri ve sunucudaki izlenmeyen `supabase/` klasörü korunur.

## Kodlanan bölümler

- Rust Worker: sağlık, secret tabanlı login ve HMAC oturumu, public kategori/ürün/detay/slider okumaları, admin katalog okuma, ürün kaydetme/silme, kategori CRUD, R2 yükleme/okuma/silme ve kullanım sayaçları.
- Web: tek `src/api.js`; public katalog/detay/slider ve admin login/katalog/kategori/slayt çağrıları Worker JSON API'sine taşındı.
- Migration: kategoriler, ürünler, özellikler, medya, slider, geçici yüklemeler ve temizleme kuyruğu tabloları.

## Yerel durum ve kalan işler

- Yerel D1 migration uygulandı ve Rust Worker `127.0.0.1:8787` üzerinde çalışıyor. Sağlık, boş kategori/ürün listesi ve web origin CORS yanıtı kontrol edildi.
- Admin oturumunu, katalog yazmalarını, kategori silme kısıtını, slider yönetimini ve taslak medya erişimini kullanıcıya ait `.dev.vars` değerleriyle yerel D1/R2 üzerinde doğrula.
- Cloudflare Pages production origin'i CORS listesine eklendi; Worker kimlik bilgileri Secrets Store binding'lerinden okunuyor.
- Cloudflare Workers Builds ortamında Cargo hazır gelmediği için Build command Rust toolchain'i kurmalı: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable --target wasm32-unknown-unknown && . "$HOME/.cargo/env" && cargo install worker-build && worker-build --release`. İlk deploy D1/R2 kaynaklarını oluşturur; üretim D1 migration'ı sonrasında uzak veritabanına uygulanmalı.
- Admin oturumunu ve CRUD işlemlerini canlı D1/R2 üzerinde henüz doğrulama; secret değerleri kullanıcı tarafından girilecek.
