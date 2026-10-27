# ESER Sunucu — Supabase ve admin planı

## Hedef

Supabase'i veri tabanı ve kimlik doğrulama sağlayıcısı olarak kullanıp ürün içeriğini güvenli biçimde sunmak; Netlify Functions üzerinden teklif formu gibi sunucu işlemlerini yürütmek. Web arayüzü ve referans sayfa taslağı `ESER-WEB/design.md` içindedir.

## Servislerin görevleri

- **Supabase Auth:** Google OAuth oturumu.
- **Supabase Postgres:** Ürünler, teknik özellikler, medya metadatası, izinli adminler ve isteğe bağlı talepler.
- **Supabase Storage:** Ürün görselleri; yalnız yayınlanan ürünlerde ziyaretçilere görünür.
- **Netlify Functions:** Teklif formu doğrulama, spam sınırlama ve güvenli kayıt/bildirim işlemleri.
- **Netlify deploy:** Web istemcisi ve Function ortam değişkenleri. Gizli anahtarlar yalnız Netlify'ın güvenli ayarlarında tutulur.

## Kimlik doğrulama ve yetkilendirme

1. Google Cloud'da OAuth istemcisi açıp callback URI olarak Supabase'in verdiği callback adresini tanımla.
2. Supabase Auth'ta Google sağlayıcısını etkinleştir; Site URL ve redirect allowlist'ini Netlify deploy URL'si, üretim özel domaini ve gereken yerel adreslerle sınırla.
3. Google OAuth yalnız kimlik doğrular. Admin yetkisi, Supabase Auth UUID'si `admin_users` tablosunda bulunan kullanıcılara verilir.
4. İlk admin UUID'si güvenilir, tek seferlik yönetici işlemiyle eklenir. Public signup, profil metadata'sı veya istemci talebi admin rolü atayamaz.
5. Admin rotaları UX seviyesinde korunur; asıl kontrol her DB yazma işleminin RLS politikasında yapılır. Oturumsuz istek reddedilir, oturumlu fakat yetkisiz istek admin tablolarına yazamaz.
6. Mümkün olduğunda kullanıcı JWT'si ile RLS kullan. `service_role` anahtarını tarayıcıya, git'e veya build çıktısına koyma; zorunlu sunucu işlemlerinde yalnız Netlify Function secret'ı olarak kullan.

## İlk veri modeli

- `products`: `id`, `slug` (benzersiz), `title`, `summary`, `content`, `status` (`draft`/`published`), `updated_at`.
- `product_specs`: `id`, `product_id`, `label`, `value`, `unit`, `sort_order`.
- `product_media`: `id`, `product_id`, `storage_path`, `alt_text`, `sort_order`.
- `admin_users`: `user_id` (Auth UUID, primary key), `created_at`.
- `leads` (gerekiyorsa): iletişim formu alanları, `created_at`, işleme durumu. Gereken en az kişisel veriyi sakla ve retention süresi belirle.

Şema değişikliklerini Supabase migration dosyalarıyla sürümle. Gerçek içerik, teknik kapasite ve iletişim verileri Eser tarafından teyit edilmeden seed etme.

## RLS ve Storage kuralları

- Anonim ve oturumlu ziyaretçiler yalnız `published` ürünleri ve bunların özellik/medya kayıtlarını okuyabilir.
- Taslaklar yalnız admin UUID'si `admin_users` içinde olan kullanıcıya görünür.
- Ürün, özellik ve medya ekleme/düzenleme/silme işlemleri yalnız adminlere açıktır.
- `admin_users` tablosunu normal kullanıcılar değiştiremez; admin kontrolü kendi kendine rol yükseltmeye izin vermez.
- Ürün görselleri için herkese açık okuma yalnız yayınlanan ürünlerle sınırlanmalı; yükleme/silme admin kontrolünde olmalı. Storage politikalarında ürün ilişkisinin doğrulanması gerekir.
- `leads` varsa anonim doğrudan SELECT/UPDATE kapalı olmalı. Insert'i Function doğrulayıp güvenli şekilde yapsın; okuma yalnız yetkili adminlere açılsın.

## Netlify Function: teklif formu

1. Girdileri sunucuda doğrula; boyut ve alan limitleri koy, HTML/script içeriğini güvenli ele al.
2. Honeypot ve/veya uygun rate limit / bot kontrolü ekle; CORS'u üretim origin'iyle sınırla.
3. Veriyi `leads` tablosuna güvenli biçimde yaz; başarı halinde kişisel veriyi yanıtta tekrar döndürme.
4. E-posta bildirimi gerekiyorsa sağlayıcıyı seçip anahtarını Netlify secret olarak ekle.
5. Loglarda form içeriği, OAuth tokenı veya secret yazdırma. Gizlilik metni ve saklama süresini ürün sahibiyle netleştir.

## Uygulama sırası

1. `ESER-WEB` kod tabanını ve teknoloji yığınını gördükten sonra Supabase client ve Netlify Functions entegrasyon biçimini belirle.
2. Supabase projesini kur; migration'ları ve RLS politikalarını ekle, önce anonim erişim senaryolarıyla doğrula.
3. Google OAuth redirect'lerini ve admin allowlist akışını kur; izinli admin, normal Google kullanıcısı ve oturumsuz erişimi dene.
4. Ürün okuma/yazma akışlarını ve görsel Storage politikalarını bağla.
5. Teklif Function'ını ekle; geçerli/geçersiz/spam gönderimleri dene.
6. Netlify preview ve production değişkenlerini ayrı tanımla; production domain redirect ve CORS ayarlarını doğrula.

## Gerekli kararlar ve güvenli ayarlar

- İlk admin olacak Google hesabı/e-posta (değerleri sohbete yazmayın; güvenilir kurulumla UUID allowlist'e eklensin).
- Üretim Netlify URL'si ve kullanılacak özel domain.
- Teklif formunun gönderileceği alıcı ve e-posta sağlayıcısı gerekip gerekmediği.
- Leads için saklama süresi ve erişim sorumlusu.
- Supabase URL ve anon/publishable key istemci değişkenleri; service-role key ve e-posta sağlayıcı anahtarı gerekiyorsa yalnız Netlify'ın korumalı Function ortamında.

## Tamamlandı ölçütleri

- Anonim kullanıcı yalnız yayınlanmış ürün içeriğini okuyabilir.
- Google ile giriş admin yetkisi vermeden çalışır; allowlist'teki admin paneli kullanabilir.
- Allowlist dışı hesap ve oturumsuz API yazma istekleri reddedilir.
- Teklif Function'ı geçerli talepleri kaydeder; bozuk ve aşırı istekleri reddeder.
- Preview/production redirect, CORS ve sır ayarları birbirine karışmaz.
