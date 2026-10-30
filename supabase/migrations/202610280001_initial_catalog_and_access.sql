create extension if not exists pgcrypto;

create table public.admin_users (
  user_id uuid primary key references auth.users (id) on delete cascade,
  created_at timestamptz not null default now()
);

create table public.products (
  id uuid primary key default gen_random_uuid(),
  slug text not null unique check (slug ~ '^[a-z0-9]+(?:-[a-z0-9]+)*$'),
  title text not null,
  summary text not null default '',
  content text not null default '',
  status text not null default 'draft' check (status in ('draft', 'published')),
  updated_at timestamptz not null default now()
);

create table public.product_specs (
  id uuid primary key default gen_random_uuid(),
  product_id uuid not null references public.products (id) on delete cascade,
  label text not null,
  value text not null,
  unit text,
  sort_order integer not null default 0
);

create table public.product_media (
  id uuid primary key default gen_random_uuid(),
  product_id uuid not null references public.products (id) on delete cascade,
  storage_path text not null unique,
  alt_text text not null default '',
  sort_order integer not null default 0
);

create index product_specs_product_sort_idx on public.product_specs (product_id, sort_order);
create index product_media_product_sort_idx on public.product_media (product_id, sort_order);

create function public.is_admin()
returns boolean
language sql
stable
security definer
set search_path = ''
as $$
  select exists (
    select 1 from public.admin_users
    where user_id = (select auth.uid())
  );
$$;

revoke all on function public.is_admin() from public;
grant execute on function public.is_admin() to authenticated;

grant select on public.products, public.product_specs, public.product_media to anon, authenticated;
grant insert, update, delete on public.products, public.product_specs, public.product_media to authenticated;
grant select on public.admin_users to authenticated;

alter table public.products enable row level security;
alter table public.product_specs enable row level security;
alter table public.product_media enable row level security;
alter table public.admin_users enable row level security;

create policy "Published products are public; admins can read all"
on public.products for select to anon, authenticated
using (status = 'published' or (select public.is_admin()));
create policy "Admins manage products"
on public.products for all to authenticated
using ((select public.is_admin()))
with check ((select public.is_admin()));

create policy "Published product specs are public; admins can read all"
on public.product_specs for select to anon, authenticated
using (
  (select public.is_admin()) or exists (
    select 1 from public.products p
    where p.id = product_id and p.status = 'published'
  )
);
create policy "Admins manage product specs"
on public.product_specs for all to authenticated
using ((select public.is_admin()))
with check ((select public.is_admin()));

create policy "Published product media metadata is public; admins can read all"
on public.product_media for select to anon, authenticated
using (
  (select public.is_admin()) or exists (
    select 1 from public.products p
    where p.id = product_id and p.status = 'published'
  )
);
create policy "Admins manage product media metadata"
on public.product_media for all to authenticated
using ((select public.is_admin()))
with check ((select public.is_admin()));

create policy "Admins can read admin allowlist"
on public.admin_users for select to authenticated
using ((select public.is_admin()));

insert into storage.buckets (id, name, public)
values ('product-media', 'product-media', false)
on conflict (id) do nothing;

create policy "Published product images are readable"
on storage.objects for select to anon, authenticated
using (
  bucket_id = 'product-media' and exists (
    select 1
    from public.product_media m
    join public.products p on p.id = m.product_id
    where m.storage_path = name
      and p.status = 'published'
  )
);
create policy "Admins can read all product images"
on storage.objects for select to authenticated
using (bucket_id = 'product-media' and (select public.is_admin()));
create policy "Admins upload product images"
on storage.objects for insert to authenticated
with check (bucket_id = 'product-media' and (select public.is_admin()));
create policy "Admins update product images"
on storage.objects for update to authenticated
using (bucket_id = 'product-media' and (select public.is_admin()))
with check (bucket_id = 'product-media' and (select public.is_admin()));
create policy "Admins delete product images"
on storage.objects for delete to authenticated
using (bucket_id = 'product-media' and (select public.is_admin()));
