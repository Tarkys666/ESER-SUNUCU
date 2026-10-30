use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use worker::*;

#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let path = req.path();
    let origin = req.headers().get("Origin")?;
    if req.method() == Method::Options {
        return cors(Response::empty()?, origin.as_deref(), &env);
    }
    let response = match (req.method(), path.as_str()) {
        (Method::Get, "/api/health") => Response::ok(r#"{"ok":true}"#),
        (Method::Get, "/api/categories") => categories(&env).await,
        (Method::Get, "/api/products") => products(&env).await,
        (Method::Get, "/api/slides") => slides(&env).await,
        (Method::Post, "/api/admin/login") => login(req, &env).await,
        (Method::Get, "/api/admin/me") => admin_me(&req, &env).await,
        (Method::Post, "/api/admin/logout") => Response::from_json(&serde_json::json!({"ok":true})),
        (Method::Get, "/api/admin/categories") => admin_categories(&req, &env).await,
        (Method::Get, "/api/admin/products") => admin_products(&req, &env).await,
        (Method::Post, "/api/admin/products") => save_product(req, &env).await,
        (Method::Get, "/api/admin/slides") => admin_slides(&req, &env).await,
        (Method::Post, "/api/admin/slides") => save_slide(req, &env).await,
        (Method::Get, "/api/admin/usage") => admin_usage(&req, &env).await,
        (Method::Post, "/api/admin/categories") => save_category(req, &env).await,
        (Method::Post, "/api/admin/media") => upload_media(req, &env).await,
        (Method::Delete, "/api/admin/media") => delete_media(&req, &env).await,
        (Method::Get, path) if path.starts_with("/api/media/") => {
            public_media(path, &req, &env).await
        }
        (Method::Delete, path) if path.starts_with("/api/admin/categories/") => {
            delete_category(path, &req, &env).await
        }
        (Method::Delete, path) if path.starts_with("/api/admin/products/") => {
            delete_product(path, req, &env).await
        }
        (Method::Put, path) if path.starts_with("/api/admin/slides/") => {
            update_slide(path, req, &env).await
        }
        (Method::Delete, path) if path.starts_with("/api/admin/slides/") => {
            delete_slide(path, req, &env).await
        }
        (Method::Get, path) if path.starts_with("/api/products/") => {
            product_detail(path, &env).await
        }
        (Method::Get, path) if path.starts_with("/api/admin/products/") => {
            admin_product(path, &req, &env).await
        }
        _ => Response::error(
            r#"{"error":{"code":"NOT_FOUND","message":"Bulunamadı"}}"#,
            404,
        ),
    }?;
    cors(response, origin.as_deref(), &env)
}

async fn products(env: &Env) -> Result<Response> {
    let rows = env.d1("DB")?.prepare("SELECT p.id,p.slug,p.title,p.category_id,p.source_url,p.summary,c.name AS category_name,p.sort_order FROM products p JOIN categories c ON c.id=p.category_id WHERE p.status='published' ORDER BY p.sort_order,p.id")
        .all().await?.results::<serde_json::Value>()?;
    let mut output = Vec::with_capacity(rows.len());
    for mut product in rows {
        let id = product["id"].as_str().unwrap_or_default();
        let media = env.d1("DB")?.prepare("SELECT id,object_key AS storage_path,alt_text,sort_order FROM product_media WHERE product_id=?1 ORDER BY sort_order,id").bind(&[id.into()])?.all().await?.results::<serde_json::Value>()?;
        if let Some(object) = product.as_object_mut() {
            object.insert(
                "product_media".into(),
                serde_json::Value::Array(media.into_iter().map(add_media_url).collect()),
            );
        }
        output.push(product);
    }
    Response::from_json(&output)
}

fn add_media_url(mut media: serde_json::Value) -> serde_json::Value {
    if let Some(object) = media.as_object_mut() {
        if let Some(key) = object.get("storage_path").and_then(|v| v.as_str()) {
            object.insert(
                "url".into(),
                serde_json::json!(format!("/api/media/{}", key)),
            );
        }
    }
    media
}

fn urlencoding(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F")
}

async fn product_detail(path: &str, env: &Env) -> Result<Response> {
    let slug = path.trim_start_matches("/api/products/");
    let Some(mut product) = env.d1("DB")?.prepare("SELECT p.id,p.slug,p.title,p.category_id,p.source_url,p.summary,p.content,p.sort_order,c.name AS category_name FROM products p JOIN categories c ON c.id=p.category_id WHERE p.slug=?1 AND p.status='published'").bind(&[slug.into()])?.first::<serde_json::Value>(None).await? else {
        return Response::error(r#"{"error":{"code":"NOT_FOUND","message":"Ürün bulunamadı"}}"#, 404);
    };
    let id = product["id"].as_str().unwrap_or_default();
    let specs = env.d1("DB")?.prepare("SELECT id,label,value,unit,sort_order FROM product_specs WHERE product_id=?1 ORDER BY sort_order,id").bind(&[id.into()])?.all().await?.results::<serde_json::Value>()?;
    let media = env.d1("DB")?.prepare("SELECT id,object_key AS storage_path,alt_text,sort_order,byte_size FROM product_media WHERE product_id=?1 ORDER BY sort_order,id").bind(&[id.into()])?.all().await?.results::<serde_json::Value>()?;
    if let Some(object) = product.as_object_mut() {
        object.insert("product_specs".into(), serde_json::Value::Array(specs));
        object.insert(
            "product_media".into(),
            serde_json::Value::Array(media.into_iter().map(add_media_url).collect()),
        );
    }
    Response::from_json(&product)
}

async fn slides(env: &Env) -> Result<Response> {
    let rows = env.d1("DB")?.prepare("SELECT id,object_key AS image_path,alt_text,sort_order FROM hero_slides WHERE is_active=1 ORDER BY sort_order,id")
        .all().await?.results::<serde_json::Value>()?;
    let rows: Vec<_> = rows
        .into_iter()
        .map(add_media_url)
        .map(|mut slide| {
            if let Some(object) = slide.as_object_mut() {
                if let Some(key) = object.get("image_path").and_then(|v| v.as_str()) {
                    object.insert(
                        "url".into(),
                        serde_json::json!(format!(
                            "/api/media/{}",
                            key.split('/')
                                .map(urlencoding)
                                .collect::<Vec<_>>()
                                .join("/")
                        )),
                    );
                }
            }
            slide
        })
        .collect();
    Response::from_json(&rows)
}

async fn login(mut req: Request, env: &Env) -> Result<Response> {
    let credentials: serde_json::Value = req.json().await?;
    let email = credentials
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let password = credentials
        .get("password")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let expected_email = required_secret(env, "ADMIN_EMAIL").await?;
    let expected_password = required_secret(env, "ADMIN_PASSWORD").await?;
    let expected_email = expected_email.trim().to_lowercase();
    if !constant_eq(email.as_bytes(), expected_email.as_bytes())
        || !constant_eq(password.as_bytes(), expected_password.as_bytes())
    {
        return Response::error(
            r#"{"error":{"code":"UNAUTHORIZED","message":"E-posta veya şifre hatalı"}}"#,
            401,
        );
    }
    let expires = (Date::now().as_millis() as u64 / 1000) + 8 * 60 * 60;
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(
        &serde_json::json!({"sub": expected_email, "exp": expires}),
    )?);
    let session_secret = required_secret(env, "SESSION_SECRET").await?;
    let mut mac = Hmac::<Sha256>::new_from_slice(session_secret.as_bytes())
        .map_err(|_| Error::RustError("Invalid session secret".into()))?;
    mac.update(payload.as_bytes());
    let token = format!(
        "{}.{}",
        payload,
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    );
    Response::from_json(
        &serde_json::json!({"token":token,"email":expected_email,"expires_at":expires}),
    )
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= (a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0)) as usize;
    }
    diff == 0
}

async fn required_secret(env: &Env, binding: &str) -> Result<String> {
    if let Ok(secret) = env.secret(binding) {
        return Ok(secret.to_string());
    }
    env.secret_store(binding)?
        .get()
        .await?
        .ok_or_else(|| Error::RustError(format!("Missing secret binding {binding}")))
}

async fn valid_session(token: &str, env: &Env) -> Result<bool> {
    let Some((payload, signature)) = token.split_once('.') else {
        return Ok(false);
    };
    let session_secret = required_secret(env, "SESSION_SECRET").await?;
    let mut mac = Hmac::<Sha256>::new_from_slice(session_secret.as_bytes())
        .map_err(|_| Error::RustError("Invalid session secret".into()))?;
    mac.update(payload.as_bytes());
    let Ok(signature) = URL_SAFE_NO_PAD.decode(signature) else {
        return Ok(false);
    };
    if mac.verify_slice(&signature).is_err() {
        return Ok(false);
    }
    let Ok(bytes) = URL_SAFE_NO_PAD.decode(payload) else {
        return Ok(false);
    };
    Ok(serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|v| v.get("exp")?.as_u64())
        .is_some_and(|exp| exp > Date::now().as_millis() as u64 / 1000))
}

async fn admin_me(req: &Request, env: &Env) -> Result<Response> {
    let Some(token) = req.headers().get("Authorization")? else {
        return Response::error(
            r#"{"error":{"code":"UNAUTHORIZED","message":"Giriş gerekli"}}"#,
            401,
        );
    };
    let Some(token) = token.strip_prefix("Bearer ") else {
        return Response::error("Unauthorized", 401);
    };
    if !valid_session(token, env).await? {
        return Response::error("Unauthorized", 401);
    }
    let email = session_email(token).unwrap_or_default();
    Response::from_json(&serde_json::json!({"authenticated":true,"email":email}))
}

fn session_email(token: &str) -> Option<String> {
    let (payload, _) = token.split_once('.')?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()?
        .get("sub")?
        .as_str()
        .map(str::to_owned)
}

async fn authorized(req: &Request, env: &Env) -> Result<bool> {
    let Some(header) = req.headers().get("Authorization")? else {
        return Ok(false);
    };
    match header.strip_prefix("Bearer ") {
        Some(token) => valid_session(token, env).await,
        None => Ok(false),
    }
}

fn unauthorized() -> Result<Response> {
    Response::error(
        r#"{"error":{"code":"UNAUTHORIZED","message":"Giriş gerekli"}}"#,
        401,
    )
}

async fn admin_categories(req: &Request, env: &Env) -> Result<Response> {
    if !authorized(req, env).await? {
        return unauthorized();
    }
    categories(env).await
}

async fn admin_products(req: &Request, env: &Env) -> Result<Response> {
    if !authorized(req, env).await? {
        return unauthorized();
    }
    let rows = env.d1("DB")?.prepare("SELECT id,slug,title,category_id,source_url,summary,status,sort_order,version FROM products ORDER BY sort_order,id")
        .all().await?.results::<serde_json::Value>()?;
    Response::from_json(&rows)
}

async fn admin_product(path: &str, req: &Request, env: &Env) -> Result<Response> {
    if !authorized(req, env).await? {
        return unauthorized();
    }
    let id = path.trim_start_matches("/api/admin/products/");
    let Some(mut product) = env.d1("DB")?.prepare("SELECT id,slug,title,category_id,source_url,summary,content,status,sort_order,version FROM products WHERE id=?1")
        .bind(&[id.into()])?.first::<serde_json::Value>(None).await? else {
        return Response::error(r#"{"error":{"code":"NOT_FOUND","message":"Ürün bulunamadı"}}"#, 404);
    };
    let specs = env.d1("DB")?.prepare("SELECT id,label,value,unit,sort_order FROM product_specs WHERE product_id=?1 ORDER BY sort_order,id").bind(&[id.into()])?.all().await?.results::<serde_json::Value>()?;
    let media = env.d1("DB")?.prepare("SELECT id,object_key AS storage_path,alt_text,sort_order,byte_size FROM product_media WHERE product_id=?1 ORDER BY sort_order,id").bind(&[id.into()])?.all().await?.results::<serde_json::Value>()?;
    if let Some(object) = product.as_object_mut() {
        object.insert("product_specs".into(), serde_json::Value::Array(specs));
        object.insert(
            "product_media".into(),
            serde_json::Value::Array(media.into_iter().map(add_media_url).collect()),
        );
    }
    Response::from_json(&product)
}

async fn admin_usage(req: &Request, env: &Env) -> Result<Response> {
    if !authorized(req, env).await? {
        return unauthorized();
    }
    let db = env.d1("DB")?;
    let product_count = db
        .prepare("SELECT COUNT(*) AS n FROM products")
        .first::<serde_json::Value>(None)
        .await?
        .and_then(|v| v["n"].as_u64())
        .unwrap_or(0);
    let category_count = db
        .prepare("SELECT COUNT(*) AS n FROM categories")
        .first::<serde_json::Value>(None)
        .await?
        .and_then(|v| v["n"].as_u64())
        .unwrap_or(0);
    let slide_count = db
        .prepare("SELECT COUNT(*) AS n FROM hero_slides")
        .first::<serde_json::Value>(None)
        .await?
        .and_then(|v| v["n"].as_u64())
        .unwrap_or(0);
    let media = db.prepare("SELECT COUNT(*) AS n,COALESCE(SUM(byte_size),0) AS bytes FROM (SELECT byte_size FROM product_media UNION ALL SELECT byte_size FROM hero_slides)").first::<serde_json::Value>(None).await?.unwrap_or_default();
    Response::from_json(
        &serde_json::json!({"product_count":product_count,"category_count":category_count,"slide_count":slide_count,"media_count":media["n"].as_u64().unwrap_or(0),"media_bytes":media["bytes"].as_u64().unwrap_or(0),"sampled_at":Date::now().to_string()}),
    )
}

async fn save_category(mut req: Request, env: &Env) -> Result<Response> {
    if !authorized(&req, env).await? {
        return unauthorized();
    }
    let data: serde_json::Value = req.json().await?;
    let id = data["id"].as_str().unwrap_or("");
    let name = data["name"].as_str().unwrap_or("").trim();
    let order = data["sort_order"].as_i64().unwrap_or(-1);
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || !(1..=150).contains(&name.chars().count())
        || !(0..=1_000_000).contains(&order)
        || id == "all"
    {
        return Response::error(
            r#"{"error":{"code":"INVALID_INPUT","message":"Kategori bilgilerini kontrol edin"}}"#,
            400,
        );
    }
    env.d1("DB")?
        .prepare("INSERT INTO categories(id,name,sort_order) VALUES(?1,?2,?3)")
        .bind(&[id.into(), name.into(), order.into()])?
        .run()
        .await?;
    Response::from_json(&serde_json::json!({"id":id,"name":name,"sort_order":order,"version":1}))
}

async fn delete_category(path: &str, req: &Request, env: &Env) -> Result<Response> {
    if !authorized(req, env).await? {
        return unauthorized();
    }
    let id = path.trim_start_matches("/api/admin/categories/");
    let exists = env
        .d1("DB")?
        .prepare("SELECT 1 FROM categories WHERE id=?1")
        .bind(&[id.into()])?
        .first::<serde_json::Value>(None)
        .await?
        .is_some();
    if !exists {
        return Response::error(
            r#"{"error":{"code":"NOT_FOUND","message":"Kategori bulunamadı"}}"#,
            404,
        );
    }
    env.d1("DB")?
        .prepare("DELETE FROM categories WHERE id=?1")
        .bind(&[id.into()])?
        .run()
        .await?;
    Response::from_json(&serde_json::json!({"deleted":true}))
}

async fn upload_media(mut req: Request, env: &Env) -> Result<Response> {
    if !authorized(&req, env).await? {
        return unauthorized();
    }
    let kind = req
        .url()?
        .query_pairs()
        .find(|(key, _)| key == "kind")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    if kind != "product" && kind != "slide" {
        return Response::error(
            r#"{"error":{"code":"INVALID_INPUT","message":"Görsel türü geçersiz"}}"#,
            400,
        );
    }
    let mime = req
        .headers()
        .get("Content-Type")?
        .unwrap_or_default()
        .to_lowercase();
    let ext = match mime.as_str() {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/avif" => "avif",
        _ => return Response::error("Unsupported image", 415),
    };
    if req
        .headers()
        .get("Content-Length")?
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|n| n > 5 * 1024 * 1024)
    {
        return Response::error("Image too large", 413);
    }
    let bytes = req.bytes().await?;
    if bytes.is_empty() || bytes.len() > 5 * 1024 * 1024 || !valid_image(&mime, &bytes) {
        return Response::error("Invalid image", 400);
    }
    let nonce = format!(
        "{:x}-{:x}",
        Date::now().as_millis(),
        (js_sys::Math::random() * 1_000_000_000.0) as u64
    );
    let key = format!(
        "{}/{}.{}",
        if kind == "slide" {
            "slides"
        } else {
            "products"
        },
        nonce,
        ext
    );
    env.bucket("MEDIA")?
        .put(&key, bytes.clone())
        .http_metadata(HttpMetadata {
            content_type: Some(mime.clone()),
            ..Default::default()
        })
        .execute()
        .await?;
    env.d1("DB")?
        .prepare("INSERT INTO staged_media(object_key,byte_size,content_type) VALUES(?1,?2,?3)")
        .bind(&[key.clone().into(), (bytes.len() as i64).into(), mime.into()])?
        .run()
        .await?;
    Response::from_json(
        &serde_json::json!({"storage_path":key,"url":format!("/api/media/{key}"),"byte_size":bytes.len()}),
    )
}

fn valid_image(mime: &str, bytes: &[u8]) -> bool {
    match mime {
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "image/png" => bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
        "image/webp" => bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        "image/avif" => {
            bytes.len() >= 12
                && &bytes[4..8] == b"ftyp"
                && (bytes.windows(4).any(|v| v == b"avif")
                    || bytes.windows(4).any(|v| v == b"avis"))
        }
        _ => false,
    }
}

async fn delete_media(req: &Request, env: &Env) -> Result<Response> {
    if !authorized(req, env).await? {
        return unauthorized();
    }
    let Some(key) = req
        .url()?
        .query_pairs()
        .find(|(name, _)| name == "key")
        .map(|(_, value)| value.to_string())
    else {
        return Response::error("Missing key", 400);
    };
    if !key.starts_with("products/") && !key.starts_with("slides/") {
        return Response::error("Invalid key", 400);
    }
    let attached = env.d1("DB")?.prepare("SELECT 1 FROM product_media WHERE object_key=?1 UNION ALL SELECT 1 FROM hero_slides WHERE object_key=?1 LIMIT 1")
        .bind(&[key.clone().into()])?.first::<serde_json::Value>(None).await?.is_some();
    if attached {
        return Response::error("Media is still in use", 409);
    }
    env.bucket("MEDIA")?.delete(key.clone()).await?;
    env.d1("DB")?
        .prepare("DELETE FROM staged_media WHERE object_key=?1")
        .bind(&[key.into()])?
        .run()
        .await?;
    Response::from_json(&serde_json::json!({"deleted":true}))
}

async fn public_media(path: &str, req: &Request, env: &Env) -> Result<Response> {
    let key = path.trim_start_matches("/api/media/");
    let public = env.d1("DB")?.prepare("SELECT 1 FROM product_media m JOIN products p ON p.id=m.product_id WHERE m.object_key=?1 AND p.status='published' UNION ALL SELECT 1 FROM hero_slides WHERE object_key=?1 AND is_active=1 LIMIT 1")
        .bind(&[key.into()])?.first::<serde_json::Value>(None).await?.is_some();
    if !public && !authorized(req, env).await? {
        return Response::error("Not found", 404);
    }
    let Some(object) = env.bucket("MEDIA")?.get(key).execute().await? else {
        return Response::error("Not found", 404);
    };
    let metadata = object.http_metadata();
    let mut response = Response::from_body(
        object
            .body()
            .ok_or_else(|| Error::RustError("Missing object body".into()))?
            .response_body()?,
    )?;
    if let Some(content_type) = metadata.content_type {
        response.headers_mut().set("Content-Type", &content_type)?;
    }
    response
        .headers_mut()
        .set("Cache-Control", "public, max-age=3600")?;
    Ok(response)
}

async fn save_product(mut req: Request, env: &Env) -> Result<Response> {
    if !authorized(&req, env).await? {
        return unauthorized();
    }
    let body: serde_json::Value = req.json().await?;
    let product = &body["product"];
    let id = product["id"].as_str().unwrap_or("");
    let category = product["category_id"].as_str().unwrap_or("");
    let slug = product["slug"].as_str().unwrap_or("");
    let title = product["title"].as_str().unwrap_or("").trim();
    let status = product["status"].as_str().unwrap_or("draft");
    let summary = product["summary"].as_str().unwrap_or("");
    let content = product["content"].as_str().unwrap_or("");
    let source = product["source_url"].as_str().unwrap_or("");
    let sort_order = product["sort_order"].as_i64().unwrap_or(0);
    let specs = body["specs"].as_array().cloned().unwrap_or_default();
    let media = body["media"].as_array().cloned().unwrap_or_default();
    if id.is_empty()
        || category.is_empty()
        || slug.is_empty()
        || slug.len() > 200
        || !(1..=250).contains(&title.chars().count())
        || summary.chars().count() > 500
        || content.chars().count() > 20_000
        || !matches!(status, "draft" | "published")
        || !(0..=1_000_000).contains(&sort_order)
        || media.len() > 12
        || specs.len() > 50
        || (!source.is_empty() && !source.starts_with("https://"))
    {
        return Response::error(
            r#"{"error":{"code":"INVALID_INPUT","message":"Ürün alanlarını kontrol edin"}}"#,
            400,
        );
    }
    let db = env.d1("DB")?;
    let current = db
        .prepare("SELECT version FROM products WHERE id=?1")
        .bind(&[id.into()])?
        .first::<serde_json::Value>(None)
        .await?;
    let expected = body["expected_version"].as_i64();
    if current.as_ref().map(|v| v["version"].as_i64().unwrap_or(0)) != expected {
        return Response::error(
            r#"{"error":{"code":"EDIT_CONFLICT","message":"Ürün başka bir oturumda değiştirildi"}}"#,
            409,
        );
    }
    let removed = db
        .prepare("SELECT object_key FROM product_media WHERE product_id=?1")
        .bind(&[id.into()])?
        .all()
        .await?
        .results::<serde_json::Value>()?;
    let mut statements = vec![db.prepare("INSERT INTO products(id,category_id,slug,title,summary,content,status,source_url,sort_order,version) VALUES(?1,?2,?3,?4,?5,?6,?7,NULLIF(?8,''),?9,1) ON CONFLICT(id) DO UPDATE SET category_id=excluded.category_id,slug=excluded.slug,title=excluded.title,summary=excluded.summary,content=excluded.content,status=excluded.status,source_url=excluded.source_url,sort_order=excluded.sort_order,version=products.version+1 WHERE products.version=?10")
        .bind(&[id.into(),category.into(),slug.into(),title.into(),summary.into(),content.into(),status.into(),source.into(),sort_order.into(),expected.unwrap_or(0).into()])?,
        db.prepare("DELETE FROM product_specs WHERE product_id=?1").bind(&[id.into()])?,
        db.prepare("DELETE FROM product_media WHERE product_id=?1").bind(&[id.into()])?];
    for (index, spec) in specs.iter().enumerate() {
        let label = spec["label"].as_str().unwrap_or("");
        let value = spec["value"].as_str().unwrap_or("");
        let unit = spec["unit"].as_str().unwrap_or("");
        if label.trim().is_empty()
            || value.trim().is_empty()
            || label.len() > 150
            || value.len() > 500
            || unit.len() > 40
        {
            return Response::error("Invalid specification", 400);
        }
        let spec_id = spec["id"].as_str().unwrap_or("");
        let generated = format!("{id}-spec-{index}");
        let spec_id = if spec_id.is_empty() {
            &generated
        } else {
            spec_id
        };
        statements.push(db.prepare("INSERT INTO product_specs(id,product_id,label,value,unit,sort_order) VALUES(?1,?2,?3,?4,?5,?6)").bind(&[spec_id.into(),id.into(),label.trim().into(),value.trim().into(),unit.into(),(index as i64).into()])?);
    }
    for (index, image) in media.iter().enumerate() {
        let key = image["storage_path"].as_str().unwrap_or("");
        let image_id = image["id"].as_str().unwrap_or("");
        let alt = image["alt_text"].as_str().unwrap_or("");
        let byte_size = image["byte_size"].as_i64().unwrap_or(0);
        if !key.starts_with("products/") || image_id.is_empty() || alt.len() > 500 || byte_size < 0
        {
            return Response::error("Invalid image metadata", 400);
        }
        statements.push(db.prepare("INSERT INTO product_media(id,product_id,object_key,alt_text,sort_order,byte_size) VALUES(?1,?2,?3,?4,?5,?6)")
            .bind(&[image_id.into(),id.into(),key.into(),alt.into(),(index as i64).into(),byte_size.into()])?);
        statements.push(
            db.prepare("DELETE FROM staged_media WHERE object_key=?1")
                .bind(&[key.into()])?,
        );
    }
    if db.batch(statements).await.is_err() {
        return Response::error(
            r#"{"error":{"code":"SAVE_FAILED","message":"Ürün kaydedilemedi; alanları veya bağlantı adını kontrol edin"}}"#,
            409,
        );
    }
    let old_keys: Vec<String> = removed
        .into_iter()
        .filter_map(|v| v["object_key"].as_str().map(str::to_owned))
        .collect();
    let next_keys: Vec<&str> = media
        .iter()
        .filter_map(|v| v["storage_path"].as_str())
        .collect();
    let removed_paths: Vec<_> = old_keys
        .into_iter()
        .filter(|key| !next_keys.contains(&key.as_str()))
        .collect();
    let saved = db.prepare("SELECT id,slug,title,category_id,source_url,summary,content,status,sort_order,version FROM products WHERE id=?1").bind(&[id.into()])?.first::<serde_json::Value>(None).await?.unwrap_or_default();
    Response::from_json(&serde_json::json!({"product":saved,"removed_paths":removed_paths}))
}

async fn delete_product(path: &str, mut req: Request, env: &Env) -> Result<Response> {
    if !authorized(&req, env).await? {
        return unauthorized();
    }
    let id = path.trim_start_matches("/api/admin/products/");
    let body: serde_json::Value = req.json().await.unwrap_or_default();
    let version = body["expected_version"].as_i64().unwrap_or(-1);
    let db = env.d1("DB")?;
    let current = db
        .prepare("SELECT version FROM products WHERE id=?1")
        .bind(&[id.into()])?
        .first::<serde_json::Value>(None)
        .await?;
    if current.as_ref().map(|v| v["version"].as_i64().unwrap_or(0)) != Some(version) {
        return Response::error("Edit conflict", 409);
    }
    let rows = db
        .prepare("SELECT object_key FROM product_media WHERE product_id=?1")
        .bind(&[id.into()])?
        .all()
        .await?
        .results::<serde_json::Value>()?;
    db.prepare("DELETE FROM products WHERE id=?1 AND version=?2")
        .bind(&[id.into(), version.into()])?
        .run()
        .await?;
    let removed_paths: Vec<_> = rows
        .into_iter()
        .filter_map(|v| v["object_key"].as_str().map(str::to_owned))
        .collect();
    Response::from_json(&serde_json::json!({"deleted":true,"removed_paths":removed_paths}))
}

async fn admin_slides(req: &Request, env: &Env) -> Result<Response> {
    if !authorized(req, env).await? {
        return unauthorized();
    }
    let rows = env.d1("DB")?.prepare("SELECT id,object_key AS image_path,alt_text,is_active,sort_order,version,byte_size FROM hero_slides ORDER BY sort_order,id")
        .all().await?.results::<serde_json::Value>()?;
    Response::from_json(
        &rows
            .into_iter()
            .map(|mut slide| {
                if let Some(object) = slide.as_object_mut() {
                    if let Some(key) = object.get("image_path").and_then(|v| v.as_str()) {
                        object.insert("url".into(), serde_json::json!(format!("/api/media/{key}")));
                    }
                }
                slide
            })
            .collect::<Vec<_>>(),
    )
}

async fn save_slide(mut req: Request, env: &Env) -> Result<Response> {
    if !authorized(&req, env).await? {
        return unauthorized();
    }
    let value: serde_json::Value = req.json().await?;
    let key = value["image_path"].as_str().unwrap_or("");
    let id = value["id"].as_str().unwrap_or("");
    let alt = value["alt_text"].as_str().unwrap_or("");
    let order = value["sort_order"].as_i64().unwrap_or(-1);
    if !key.starts_with("slides/")
        || id.is_empty()
        || alt.len() > 500
        || !(0..=1_000_000).contains(&order)
    {
        return Response::error("Invalid slide", 400);
    }
    env.d1("DB")?.prepare("INSERT INTO hero_slides(id,object_key,alt_text,is_active,sort_order,byte_size) VALUES(?1,?2,?3,?4,?5,COALESCE((SELECT byte_size FROM staged_media WHERE object_key=?2),0))")
        .bind(&[id.into(),key.into(),alt.into(),(if value["is_active"].as_bool().unwrap_or(true) {1_i64} else {0_i64}).into(),order.into()])?.run().await?;
    env.d1("DB")?
        .prepare("DELETE FROM staged_media WHERE object_key=?1")
        .bind(&[key.into()])?
        .run()
        .await?;
    Response::from_json(
        &serde_json::json!({"id":id,"image_path":key,"alt_text":alt,"is_active":value["is_active"].as_bool().unwrap_or(true),"sort_order":order,"version":1}),
    )
}

async fn update_slide(path: &str, mut req: Request, env: &Env) -> Result<Response> {
    if !authorized(&req, env).await? {
        return unauthorized();
    }
    let id = path.trim_start_matches("/api/admin/slides/");
    let value: serde_json::Value = req.json().await?;
    let key = value["image_path"].as_str().unwrap_or("");
    let alt = value["alt_text"].as_str().unwrap_or("");
    let order = value["sort_order"].as_i64().unwrap_or(-1);
    let version = value["expected_version"].as_i64().unwrap_or(0);
    let active = if value["is_active"].as_bool().unwrap_or(false) {
        1_i64
    } else {
        0_i64
    };
    if !key.starts_with("slides/") || alt.len() > 500 || !(0..=1_000_000).contains(&order) {
        return Response::error("Invalid slide", 400);
    }
    let result = env.d1("DB")?.prepare("UPDATE hero_slides SET object_key=?1,alt_text=?2,is_active=?3,sort_order=?4,byte_size=COALESCE((SELECT byte_size FROM staged_media WHERE object_key=?1),byte_size),version=version+1 WHERE id=?5 AND version=?6")
        .bind(&[key.into(),alt.into(),active.into(),order.into(),id.into(),version.into()])?.run().await?;
    if !result
        .meta()?
        .is_some_and(|meta| meta.changes.unwrap_or(0) > 0)
    {
        return Response::error("Edit conflict", 409);
    }
    env.d1("DB")?
        .prepare("DELETE FROM staged_media WHERE object_key=?1")
        .bind(&[key.into()])?
        .run()
        .await?;
    Response::from_json(
        &serde_json::json!({"id":id,"image_path":key,"alt_text":alt,"is_active":active==1,"sort_order":order,"version":version+1}),
    )
}

async fn delete_slide(path: &str, mut req: Request, env: &Env) -> Result<Response> {
    if !authorized(&req, env).await? {
        return unauthorized();
    }
    let id = path.trim_start_matches("/api/admin/slides/");
    let value: serde_json::Value = req.json().await.unwrap_or_default();
    let version = value["expected_version"].as_i64().unwrap_or(0);
    let row = env
        .d1("DB")?
        .prepare("SELECT object_key FROM hero_slides WHERE id=?1 AND version=?2")
        .bind(&[id.into(), version.into()])?
        .first::<serde_json::Value>(None)
        .await?;
    let Some(row) = row else {
        return Response::error("Edit conflict", 409);
    };
    env.d1("DB")?
        .prepare("DELETE FROM hero_slides WHERE id=?1 AND version=?2")
        .bind(&[id.into(), version.into()])?
        .run()
        .await?;
    Response::from_json(&serde_json::json!({"deleted":true,"removed_paths":[row["object_key"]]}))
}

async fn categories(env: &Env) -> Result<Response> {
    let rows = env
        .d1("DB")?
        .prepare("SELECT id,name,sort_order FROM categories ORDER BY sort_order,id")
        .all()
        .await?
        .results::<serde_json::Value>()?;
    Response::from_json(&rows)
}

fn cors(mut response: Response, origin: Option<&str>, env: &Env) -> Result<Response> {
    let allowed = env.var("ALLOWED_ORIGINS")?.to_string();
    if let Some(origin) = origin.filter(|origin| allowed.split(',').any(|item| item == *origin)) {
        response
            .headers_mut()
            .set("Access-Control-Allow-Origin", origin)?;
        response.headers_mut().set("Vary", "Origin")?;
        response.headers_mut().set(
            "Access-Control-Allow-Headers",
            "Authorization, Content-Type",
        )?;
        response.headers_mut().set(
            "Access-Control-Allow-Methods",
            "GET, POST, PUT, DELETE, OPTIONS",
        )?;
    }
    Ok(response)
}
