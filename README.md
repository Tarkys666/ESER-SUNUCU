# Dilek Makina Worker (yerel)

Gerekenler: Rust + `wasm32-unknown-unknown`, Node.js 22+ ve pnpm.

```powershell
pnpm install
cargo install worker-build
Copy-Item .dev.vars.example .dev.vars
# .dev.vars içindeki admin e-posta/şifresini ve uzun rastgele SESSION_SECRET değerini düzenle.
pnpm db:migrate:local
pnpm dev
```

Worker `http://127.0.0.1:8787` adresinde açılır. D1/R2 verisi `.wrangler/` altında yerel tutulur. `wrangler.local.toml` ve migration komutundaki `--local` uzak Cloudflare hesabına erişmez. `.dev.vars` gizlidir ve Git'e eklenmez.

Frontend `ESER WEB` reposunda `http://127.0.0.1:4173` üzerinde çalışır. Cloudflare Pages production origin'leri `https://eser-web.pages.dev` ve `https://dilekmakina.net` olarak CORS listesine eklendi.

Cloudflare Secrets Store'da `ADMIN_EMAIL`, `ADMIN_PASSWORD` ve uzun, rastgele `SESSION_SECRET` adlarında, `workers` kapsamlı üç secret oluşturulmalıdır. Secret değerleri repoya veya sohbete yazılmaz. Yerelde aynı adları `.dev.vars` içine koy.

Cloudflare Workers Builds ortamında Rust kurulu gelmediğinden Build command önce Rust toolchain'i kurmalıdır: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable --target wasm32-unknown-unknown && . "$HOME/.cargo/env" && cargo install worker-build && worker-build --release`. Deploy command `npx wrangler deploy` olmalıdır. İlk başarılı dağıtım D1/R2 kaynaklarını oluşturur; üretim D1 şemasını ayrıca Cloudflare panelindeki gerçek veritabanı adıyla `pnpm exec wrangler d1 migrations apply <veritabanı-adı> --remote` komutuyla uygula.
