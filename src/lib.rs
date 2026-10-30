use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use worker::*;

const PUBLIC_MEDIA_CACHE_CONTROL: &str = "public, max-age=31536000, s-maxage=31536000, immutable";

#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let path = req.path();
    let method = req.method();
    let origin = req.headers().get("Origin")?;
    if req.method() == Method::Options {
        return cors(Response::empty()?, origin.as_deref(), &env);
    }
    let cache_key = public_cache_key(&req, &path, origin.as_deref(), &env)?;
    if let Some(key) = cache_key.as_deref() {
        if let Ok(Some(response)) = Cache::default().get(key, false).await {
            return Ok(response);
        }
    }
    let mut response = match (method.clone(), path.as_str()) {
        (Method::Get, "/api/health") => Response::ok(r#"{"ok":true}"#),
        (Method::Get, "/api/home") => home(&env).await,
        (Method::Get, "/api/categories") => categories(&env).await,
        (Method::Get, "/api/products") => products(&req, &env).await,
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
        (Method::Put, path) if path.starts_with("/api/admin/categories/") => {
            update_category(path, req, &env).await
        }
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
    }.unwrap_or_else(|_| {
        Response::error(r#"{"error":{"code":"INTERNAL_ERROR","message":"İşlem tamamlanamadı. Lütfen tekrar deneyin."}}"#, 500).unwrap()
    });
    if !path.starts_with("/api/media/") || response.status_code() >= 400 {
        response.headers_mut().set("Cache-Control", "no-store")?;
    }
    response = cors(response, origin.as_deref(), &env)?;
    if let Some(key) = cache_key.filter(|_| response.status_code() == 200) {
        if let Ok(mut cacheable) = response.cloned() {
            let cache_control = if path.starts_with("/api/media/") {
                PUBLIC_MEDIA_CACHE_CONTROL
            } else {
                "public, max-age=0, s-maxage=60"
            };
            let _ = cacheable.headers_mut().set("Cache-Control", cache_control);
            let _ = Cache::default().put(key, cacheable).await;
        }
    }
    Ok(response)
}

fn public_cache_key(
    req: &Request,
    path: &str,
    origin: Option<&str>,
    env: &Env,
) -> Result<Option<String>> {
    if req.method() != Method::Get || req.headers().get("Authorization")?.is_some() {
        return Ok(None);
    }
    let url = req.url()?;
    let query = url.query();
    // Cache immutable media only; catalog reads must reflect admin writes globally.
    let allowed = path.starts_with("/api/media/") && query.is_none();
    if !allowed {
        return Ok(None);
    }
    let allowed_origins = env.var("ALLOWED_ORIGINS")?.to_string();
    let cache_origin = origin.filter(|value| allowed_origins.split(',').any(|item| item == *value));
    let query_string = query.map(|q| format!("?{q}")).unwrap_or_default();
    let origin_suffix = cache_origin
        .map(|value| {
            format!(
                "{}__origin={}",
                if query_string.is_empty() { "?" } else { "&" },
                urlencoding(value)
            )
        })
        .unwrap_or_default();
    Ok(Some(format!(
        "https://dilekmakina-cache.internal/v2{path}{query_string}{origin_suffix}"
    )))
}

async fn products(req: &Request, env: &Env) -> Result<Response> {
    let params: std::collections::HashMap<String, String> = req.query()?;
    let page = params
        .get("page")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1)
        .clamp(1, 100_000);
    let category = params.get("category").map(String::as_str).unwrap_or("");
    let query = params
        .get("q")
        .map(|s| s.chars().take(100).collect::<String>())
        .unwrap_or_default();
    let search = format!("%{query}%");
    let db = env.d1("DB")?;
    let total = db
        .prepare(
            "SELECT COUNT(*) AS n FROM products WHERE status='published' AND (?1='' OR category_id=?1) AND (?2='' OR title LIKE ?3 OR summary LIKE ?3 OR category_id IN (SELECT id FROM categories WHERE name LIKE ?3))",
        )
        .bind(&[category.into(), query.as_str().into(), search.as_str().into()])?
        .first::<serde_json::Value>(None)
        .await?
        .and_then(|row| row["n"].as_u64())
        .unwrap_or(0);
    let offset = (page - 1) * 24;
    let rows = db
        .prepare(
            "SELECT p.id,p.slug,p.title,p.category_id,p.source_url,p.summary,c.name AS category_name,p.sort_order,m.id AS media_id,m.object_key AS storage_path,m.alt_text,m.sort_order AS media_sort FROM products p JOIN categories c ON c.id=p.category_id LEFT JOIN product_media m ON m.id=(SELECT pm.id FROM product_media pm WHERE pm.product_id=p.id ORDER BY pm.sort_order,pm.id LIMIT 1) WHERE p.status='published' AND (?1='' OR p.category_id=?1) AND (?2='' OR p.title LIKE ?3 OR p.summary LIKE ?3 OR c.name LIKE ?3) ORDER BY p.sort_order,p.id LIMIT 24 OFFSET ?4",
        )
        .bind(&[
            category.into(),
            query.as_str().into(),
            search.as_str().into(),
            (offset as i32).into(),
        ])?
        .all()
        .await?
        .results::<serde_json::Value>()?;
    let items: Vec<_> = rows
        .into_iter()
        .map(|mut row| {
            let media = if row["media_id"].is_string() {
                let media = serde_json::json!({"id":row["media_id"],"storage_path":row["storage_path"],"alt_text":row["alt_text"],"sort_order":row["media_sort"]});
                vec![add_media_url(media)]
            } else {
                Vec::new()
            };
            if let Some(object) = row.as_object_mut() {
                for key in ["media_id", "storage_path", "alt_text", "media_sort"] {
                    object.remove(key);
                }
                object.insert("product_media".into(), serde_json::Value::Array(media));
            }
            row
        })
        .collect();
    Response::from_json(
        &serde_json::json!({"items":items,"total":total,"page":page,"page_size":24}),
    )
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
    let Some(mut product) = env.d1("DB")?.prepare("SELECT p.id,p.slug,p.title,p.category_id,p.source_url,p.summary,p.content,p.trendyol_url,p.idefix_url,p.hepsiburada_url,p.pazarama_url,p.n11_url,p.sort_order,c.name AS category_name FROM products p JOIN categories c ON c.id=p.category_id WHERE p.slug=?1 AND p.status='published'").bind(&[slug.into()])?.first::<serde_json::Value>(None).await? else {
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

async fn home(env: &Env) -> Result<Response> {
    let categories = env
        .d1("DB")?
        .prepare(
            r#"SELECT c.id,c.name,c.sort_order,
                      COALESCE(selected.id,fallback.id) AS product_id,
                      COALESCE(selected.slug,fallback.slug) AS product_slug,
                      COALESCE(selected.title,fallback.title) AS product_title,
                      COALESCE(selected_media.object_key,fallback_media.object_key) AS image_path,
                      COALESCE(selected_media.alt_text,fallback_media.alt_text) AS alt_text
               FROM categories c
               LEFT JOIN products selected
                 ON selected.id=c.featured_product_id AND selected.category_id=c.id AND selected.status='published'
               LEFT JOIN products fallback ON fallback.id=(
                 SELECT p.id FROM products p
                 WHERE p.category_id=c.id AND p.status='published'
                 ORDER BY CASE WHEN EXISTS(SELECT 1 FROM product_media pm WHERE pm.product_id=p.id) THEN 0 ELSE 1 END,
                          p.sort_order,p.id
                 LIMIT 1
               )
               LEFT JOIN product_media selected_media ON selected_media.id=(
                 SELECT pm.id FROM product_media pm WHERE pm.product_id=selected.id ORDER BY pm.sort_order,pm.id LIMIT 1
               )
               LEFT JOIN product_media fallback_media ON fallback_media.id=(
                 SELECT pm.id FROM product_media pm WHERE pm.product_id=fallback.id ORDER BY pm.sort_order,pm.id LIMIT 1
               )
               ORDER BY c.sort_order,c.id"#,
        )
        .all().await?.results::<serde_json::Value>()?;
    let categories: Vec<_> = categories.into_iter().map(|mut category| {
        if category["product_id"].is_string() {
            let product = serde_json::json!({"id":category["product_id"],"slug":category["product_slug"],"title":category["product_title"],"image_path":category["image_path"],"alt_text":category["alt_text"]});
            if let Some(object) = category.as_object_mut() {
                for key in ["product_id", "product_slug", "product_title", "image_path", "alt_text"] { object.remove(key); }
                object.insert("featured_product".into(), product);
            }
        } else if let Some(object) = category.as_object_mut() {
            for key in ["product_id", "product_slug", "product_title", "image_path", "alt_text"] { object.remove(key); }
            object.insert("featured_product".into(), serde_json::Value::Null);
        }
        if let Some(product) = category.get_mut("featured_product").filter(|v| !v.is_null()) {
            if let Some(key) = product.get("image_path").and_then(|v| v.as_str()).map(str::to_owned) {
                if let Some(object) = product.as_object_mut() { object.insert("url".into(), serde_json::json!(format!("/api/media/{}", key.split('/').map(urlencoding).collect::<Vec<_>>().join("/")))); }
            }
        }
        category
    }).collect();
    let slides = env.d1("DB")?.prepare("SELECT id,object_key AS image_path,alt_text,sort_order FROM hero_slides WHERE is_active=1 ORDER BY sort_order,id")
        .all().await?.results::<serde_json::Value>()?;
    let slides: Vec<_> = slides
        .into_iter()
        .map(|mut slide| {
            if let Some(key) = slide["image_path"].as_str().map(str::to_owned) {
                if let Some(object) = slide.as_object_mut() {
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
    Response::from_json(&serde_json::json!({"categories":categories,"slides":slides}))
}

async fn login(mut req: Request, env: &Env) -> Result<Response> {
    let ip = req
        .headers()
        .get("CF-Connecting-IP")?
        .unwrap_or_else(|| "local".to_owned());
    let session_secret = required_secret(env, "SESSION_SECRET").await?;
    let mut ip_mac = Hmac::<Sha256>::new_from_slice(session_secret.as_bytes())
        .map_err(|_| Error::RustError("Invalid session secret".into()))?;
    ip_mac.update(b"admin-login-ip:");
    ip_mac.update(ip.as_bytes());
    let ip_key = URL_SAFE_NO_PAD.encode(ip_mac.finalize().into_bytes());
    let now = (Date::now().as_millis() as u64 / 1000) as i64;
    let db = env.d1("DB")?;
    let locked_until = db
        .prepare("SELECT locked_until FROM admin_login_locks WHERE ip_key=?1")
        .bind(&[ip_key.as_str().into()])?
        .first::<serde_json::Value>(None)
        .await?
        .and_then(|row| row["locked_until"].as_i64());
    if let Some(until) = locked_until.filter(|until| *until > now) {
        let mut response = Response::error(
            r#"{"error":{"code":"LOGIN_LOCKED","message":"Çok fazla hatalı giriş. 10 dakika sonra tekrar deneyin."}}"#,
            429,
        )?;
        response
            .headers_mut()
            .set("Retry-After", &(until - now).to_string())?;
        return Ok(response);
    }
    db.prepare("DELETE FROM admin_login_locks WHERE locked_until<=?1")
        .bind(&[now.into()])?
        .run()
        .await?;

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
        db.prepare("INSERT INTO admin_login_locks(ip_key,locked_until) VALUES(?1,?2) ON CONFLICT(ip_key) DO UPDATE SET locked_until=excluded.locked_until")
            .bind(&[ip_key.as_str().into(), (now + 600).into()])?
            .run()
            .await?;
        return Response::error(
            r#"{"error":{"code":"UNAUTHORIZED","message":"E-posta veya şifre hatalı"}}"#,
            401,
        );
    }
    db.prepare("DELETE FROM admin_login_locks WHERE ip_key=?1")
        .bind(&[ip_key.as_str().into()])?
        .run()
        .await?;
    let expires = (Date::now().as_millis() as u64 / 1000) + 8 * 60 * 60;
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(
        &serde_json::json!({"sub": expected_email, "exp": expires}),
    )?);
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
    let Some(mut product) = env.d1("DB")?.prepare("SELECT id,slug,title,category_id,source_url,summary,content,trendyol_url,idefix_url,hepsiburada_url,pazarama_url,n11_url,status,sort_order,version FROM products WHERE id=?1")
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
        .bind(&[id.into(), name.into(), (order as i32).into()])?
        .run()
        .await?;
    Response::from_json(&serde_json::json!({"id":id,"name":name,"sort_order":order,"version":1}))
}

async fn update_category(path: &str, mut req: Request, env: &Env) -> Result<Response> {
    if !authorized(&req, env).await? {
        return unauthorized();
    }
    let id = path.trim_start_matches("/api/admin/categories/");
    let body: serde_json::Value = req.json().await?;
    let product_id = body["featured_product_id"].as_str().unwrap_or("");
    if !product_id.is_empty() {
        let valid = env
            .d1("DB")?
            .prepare("SELECT 1 FROM products WHERE id=?1 AND category_id=?2 AND status='published'")
            .bind(&[product_id.into(), id.into()])?
            .first::<serde_json::Value>(None)
            .await?
            .is_some();
        if !valid {
            return Response::error(
                r#"{"error":{"code":"INVALID_FEATURED_PRODUCT","message":"Yalnızca aynı kategorideki yayındaki bir ürün seçilebilir"}}"#,
                400,
            );
        }
    }
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
        .prepare("UPDATE categories SET featured_product_id=NULLIF(?2,'') WHERE id=?1")
        .bind(&[id.into(), product_id.into()])?
        .run()
        .await?;
    Response::from_json(
        &serde_json::json!({"id":id,"featured_product_id":if product_id.is_empty(){serde_json::Value::Null}else{serde_json::json!(product_id)}}),
    )
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
    let db = env.d1("DB")?;
    let in_use = db.prepare("SELECT 1 FROM products WHERE category_id=?1 LIMIT 1")
        .bind(&[id.into()])?.first::<serde_json::Value>(None).await?.is_some();
    if in_use {
        return Response::error(r#"{"error":{"code":"CATEGORY_IN_USE","message":"Bu kategoride ürün var. Önce ürünleri başka kategoriye taşıyın veya silin."}}"#, 409);
    }
    db.prepare("DELETE FROM categories WHERE id=?1")
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
    let byte_size = bytes.len();
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
        .put(&key, bytes)
        .http_metadata(HttpMetadata {
            content_type: Some(mime.clone()),
            ..Default::default()
        })
        .execute()
        .await?;
    env.d1("DB")?
        .prepare("INSERT INTO staged_media(object_key,byte_size,content_type) VALUES(?1,?2,?3)")
        .bind(&[key.clone().into(), (byte_size as i32).into(), mime.into()])?
        .run()
        .await?;
    Response::from_json(
        &serde_json::json!({"storage_path":key,"url":format!("/api/media/{key}"),"byte_size":byte_size}),
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
                && bytes[8..]
                    .windows(4)
                    .any(|v| v == b"avif" || v == b"avis")
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
    response.headers_mut().set(
        "Cache-Control",
        if public {
            PUBLIC_MEDIA_CACHE_CONTROL
        } else {
            "private, no-store"
        },
    )?;
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
    let trendyol = product["trendyol_url"].as_str().unwrap_or("");
    let idefix = product["idefix_url"].as_str().unwrap_or("");
    let hepsiburada = product["hepsiburada_url"].as_str().unwrap_or("");
    let pazarama = product["pazarama_url"].as_str().unwrap_or("");
    let n11 = product["n11_url"].as_str().unwrap_or("");
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
        || [trendyol, idefix, hepsiburada, pazarama, n11]
            .iter()
            .any(|url| !url.is_empty() && (url.len() > 2048 || !url.starts_with("https://")))
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
    let mut statements = vec![db.prepare("INSERT INTO products(id,category_id,slug,title,summary,content,status,source_url,sort_order,trendyol_url,idefix_url,hepsiburada_url,pazarama_url,n11_url,version) VALUES(?1,?2,?3,?4,?5,?6,?7,NULLIF(?8,''),?9,NULLIF(?10,''),NULLIF(?11,''),NULLIF(?12,''),NULLIF(?13,''),NULLIF(?14,''),1) ON CONFLICT(id) DO UPDATE SET category_id=excluded.category_id,slug=excluded.slug,title=excluded.title,summary=excluded.summary,content=excluded.content,status=excluded.status,source_url=excluded.source_url,sort_order=excluded.sort_order,trendyol_url=excluded.trendyol_url,idefix_url=excluded.idefix_url,hepsiburada_url=excluded.hepsiburada_url,pazarama_url=excluded.pazarama_url,n11_url=excluded.n11_url,version=products.version+1 WHERE products.version=?15")
        .bind(&[id.into(),category.into(),slug.into(),title.into(),summary.into(),content.into(),status.into(),source.into(),(sort_order as i32).into(),trendyol.into(),idefix.into(),hepsiburada.into(),pazarama.into(),n11.into(),(expected.unwrap_or(0) as i32).into()])?,
        db.prepare("UPDATE categories SET featured_product_id=NULL WHERE featured_product_id=?1 AND NOT EXISTS(SELECT 1 FROM products p WHERE p.id=?1 AND p.category_id=categories.id AND p.status='published')").bind(&[id.into()])?,
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
        statements.push(db.prepare("INSERT INTO product_specs(id,product_id,label,value,unit,sort_order) VALUES(?1,?2,?3,?4,?5,?6)").bind(&[spec_id.into(),id.into(),label.trim().into(),value.trim().into(),unit.into(),(index as i32).into()])?);
    }
    for (index, image) in media.iter().enumerate() {
        let key = image["storage_path"].as_str().unwrap_or("");
        let image_id = image["id"].as_str().unwrap_or("");
        let alt = image["alt_text"].as_str().unwrap_or("");
        let byte_size = image["byte_size"].as_i64().unwrap_or(0);
        if !key.starts_with("products/")
            || image_id.is_empty()
            || alt.len() > 500
            || !(0..=i32::MAX as i64).contains(&byte_size)
        {
            return Response::error("Invalid image metadata", 400);
        }
        statements.push(db.prepare("INSERT INTO product_media(id,product_id,object_key,alt_text,sort_order,byte_size) VALUES(?1,?2,?3,?4,?5,?6)")
            .bind(&[image_id.into(),id.into(),key.into(),alt.into(),(index as i32).into(),(byte_size as i32).into()])?);
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
        .bind(&[id.into(), (version as i32).into()])?
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
        .bind(&[id.into(),key.into(),alt.into(),(if value["is_active"].as_bool().unwrap_or(true) {1_i32} else {0_i32}).into(),(order as i32).into()])?.run().await?;
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
        1_i32
    } else {
        0_i32
    };
    if !key.starts_with("slides/") || alt.len() > 500 || !(0..=1_000_000).contains(&order) {
        return Response::error("Invalid slide", 400);
    }
    let result = env.d1("DB")?.prepare("UPDATE hero_slides SET object_key=?1,alt_text=?2,is_active=?3,sort_order=?4,byte_size=COALESCE((SELECT byte_size FROM staged_media WHERE object_key=?1),byte_size),version=version+1 WHERE id=?5 AND version=?6")
        .bind(&[key.into(),alt.into(),active.into(),(order as i32).into(),id.into(),(version as i32).into()])?.run().await?;
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
        .bind(&[id.into(), (version as i32).into()])?
        .first::<serde_json::Value>(None)
        .await?;
    let Some(row) = row else {
        return Response::error("Edit conflict", 409);
    };
    env.d1("DB")?
        .prepare("DELETE FROM hero_slides WHERE id=?1 AND version=?2")
        .bind(&[id.into(), (version as i32).into()])?
        .run()
        .await?;
    Response::from_json(&serde_json::json!({"deleted":true,"removed_paths":[row["object_key"]]}))
}

async fn categories(env: &Env) -> Result<Response> {
    let rows = env
        .d1("DB")?
        .prepare(
            "SELECT id,name,sort_order,featured_product_id FROM categories ORDER BY sort_order,id",
        )
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
