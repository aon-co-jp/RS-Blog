//! # RS-Blog (v0.1.0)
//!
//! [WordPress](https://wordpress.org/)(実際にはPHP製)の、
//! ハイスピード・ハイセキュリティ・省メモリなRust+[poem](https://github.com/poem-web/poem)版を目指す。
//!
//! ## 正直な開示(最重要、`RGit`/`RS-Chiketto`/`aruaru-llm`と同じ流儀)
//!
//! **v0.1.0時点では、投稿(Post)のCRUDのみ実装している。**
//! WordPressが持つ以下の機能は**まだ一切無い**:
//!
//! - カスタム投稿タイプ
//! - テーマ・ウィジェット
//! - プラグイン機構(PHPプラグイン互換レイヤも未着手)
//! - メディアライブラリ
//! - ユーザー・ロール・権限管理(登録アカウント制・アクセス制御の細分化)
//!
//! 認証は[`RGit`](https://github.com/aon-co-jp/RGit)/[`RS-Chiketto`](https://github.com/aon-co-jp/RS-Chiketto)で
//! 先行実装したOTPログイン(固定管理者のみ)をそのまま移植して使用。
//! ストレージは現時点でJSONファイル永続化(`aruaru-db`/PostgreSQL
//! DUAL DB構成への移行は未着手、`CLAUDE.md`のHANDOFF参照)。

mod auth;
mod mail;

use std::path::PathBuf;
use std::sync::Arc;

use poem::listener::TcpListener;
use poem::middleware::Tracing;
use poem::web::Data;
use poem::{
    delete, get, handler, post,
    web::Path as PathExtractor,
    EndpointExt, Request, Response, Result as PoemResult, Route, Server,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState {
    data_root: PathBuf,
    auth: Arc<auth::AuthStore>,
    admin_email: String,
    smtp: Option<mail::SmtpConfig>,
}

fn require_admin_session(req: &Request, state: &AppState) -> PoemResult<()> {
    let header = req.header(poem::http::header::AUTHORIZATION).unwrap_or("");
    let token = header.strip_prefix("Bearer ").unwrap_or("");
    match state.auth.session_email(token) {
        Some(email) if email == state.admin_email => Ok(()),
        _ => Err(poem::Error::from_string("admin login required", poem::http::StatusCode::UNAUTHORIZED)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum PostStatus {
    Draft,
    Published,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Post {
    id: u64,
    title: String,
    body: String,
    status: PostStatus,
    #[serde(default)]
    categories: Vec<u64>,
    #[serde(default)]
    tags: Vec<u64>,
    created_at: u64,
    updated_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PostStore {
    next_id: u64,
    posts: Vec<Post>,
}

fn posts_path(data_root: &std::path::Path) -> PathBuf {
    data_root.join("posts.json")
}

async fn load_posts(data_root: &std::path::Path) -> PostStore {
    match tokio::fs::read(posts_path(data_root)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => PostStore::default(),
    }
}

async fn save_posts(data_root: &std::path::Path, store: &PostStore) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(store).expect("PostStore serialization is infallible");
    tokio::fs::write(posts_path(data_root), bytes).await
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[derive(Deserialize)]
struct CreatePostRequest {
    title: String,
    body: String,
    #[serde(default)]
    categories: Vec<u64>,
    #[serde(default)]
    tags: Vec<u64>,
}

/// `POST /api/posts` — 投稿を新規作成する(ログイン必須、初期状態は`draft`)。
/// v0.1.0時点では固定ページ・カスタム投稿タイプは未実装、全投稿が
/// 単一のフラットな一覧に入る(次の増分で対応、CLAUDE.md参照)。
#[handler]
async fn create_post(req: &Request, state: Data<&AppState>, body: poem::web::Json<CreatePostRequest>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    if body.title.trim().is_empty() {
        return Ok(Response::builder().status(poem::http::StatusCode::BAD_REQUEST).body("title must not be empty"));
    }
    let mut store = load_posts(&state.data_root).await;
    let id = store.next_id;
    store.next_id += 1;
    let now = now_unix();
    let post = Post {
        id,
        title: body.title.clone(),
        body: body.body.clone(),
        status: PostStatus::Draft,
        categories: body.categories.clone(),
        tags: body.tags.clone(),
        created_at: now,
        updated_at: now,
    };
    store.posts.push(post.clone());
    save_posts(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder()
        .status(poem::http::StatusCode::CREATED)
        .content_type("application/json")
        .body(serde_json::to_vec(&post).unwrap_or_default()))
}

/// `GET /api/posts` — 投稿一覧(ログイン必須、v0.1.0時点では
/// 閲覧範囲の絞り込みは無く管理者のみが閲覧可能——公開ページとしての
/// 未ログイン閲覧は次の増分で追加する)。
#[handler]
async fn list_posts(req: &Request, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let store = load_posts(&state.data_root).await;
    // シンプルなクエリパース(`?category=<id>`・`?tag=<id>`に対応)。
    let category_filter: Option<u64> = req.uri().query().and_then(|q| {
        q.split('&').find_map(|pair| pair.strip_prefix("category=").and_then(|v| v.parse::<u64>().ok()))
    });
    let tag_filter: Option<u64> = req.uri().query().and_then(|q| {
        q.split('&').find_map(|pair| pair.strip_prefix("tag=").and_then(|v| v.parse::<u64>().ok()))
    });
    let posts: Vec<&Post> = store
        .posts
        .iter()
        .filter(|p| category_filter.map(|cat_id| p.categories.contains(&cat_id)).unwrap_or(true))
        .filter(|p| tag_filter.map(|tag_id| p.tags.contains(&tag_id)).unwrap_or(true))
        .collect();
    Ok(Response::builder()
        .status(poem::http::StatusCode::OK)
        .content_type("application/json")
        .body(serde_json::to_vec(&posts).unwrap_or_default()))
}

#[handler]
async fn get_post(req: &Request, PathExtractor(id): PathExtractor<u64>, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let store = load_posts(&state.data_root).await;
    match store.posts.iter().find(|p| p.id == id) {
        Some(post) => Ok(Response::builder()
            .status(poem::http::StatusCode::OK)
            .content_type("application/json")
            .body(serde_json::to_vec(post).unwrap_or_default())),
        None => Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("post not found")),
    }
}

#[derive(Deserialize)]
struct UpdatePostRequest {
    title: Option<String>,
    body: Option<String>,
    status: Option<PostStatus>,
    categories: Option<Vec<u64>>,
    tags: Option<Vec<u64>>,
}

/// `PUT /api/posts/:id` — 投稿のタイトル・本文・ステータスを更新する
/// (ログイン必須、指定したフィールドのみ更新)。
#[handler]
async fn update_post(
    req: &Request,
    PathExtractor(id): PathExtractor<u64>,
    state: Data<&AppState>,
    body: poem::web::Json<UpdatePostRequest>,
) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let mut store = load_posts(&state.data_root).await;
    let Some(post) = store.posts.iter_mut().find(|p| p.id == id) else {
        return Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("post not found"));
    };
    if let Some(title) = &body.title {
        post.title = title.clone();
    }
    if let Some(post_body) = &body.body {
        post.body = post_body.clone();
    }
    if let Some(status) = &body.status {
        post.status = status.clone();
    }
    if let Some(categories) = &body.categories {
        post.categories = categories.clone();
    }
    if let Some(tags) = &body.tags {
        post.tags = tags.clone();
    }
    post.updated_at = now_unix();
    let updated = post.clone();
    save_posts(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder()
        .status(poem::http::StatusCode::OK)
        .content_type("application/json")
        .body(serde_json::to_vec(&updated).unwrap_or_default()))
}

/// `DELETE /api/posts/:id` — 投稿を削除する(ログイン必須)。
#[handler]
async fn delete_post(req: &Request, PathExtractor(id): PathExtractor<u64>, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let mut store = load_posts(&state.data_root).await;
    let before = store.posts.len();
    store.posts.retain(|p| p.id != id);
    if store.posts.len() == before {
        return Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("post not found"));
    }
    save_posts(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder().status(poem::http::StatusCode::OK).body("deleted"))
}

// ---------------------------------------------------------------------
// カテゴリ(Category) — 単純なCRUD、管理者のみ。投稿は`categories: Vec<u64>`
// でカテゴリIDを参照する(多対多)。
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Category {
    id: u64,
    name: String,
    slug: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CategoryStore {
    next_id: u64,
    categories: Vec<Category>,
}

fn categories_path(data_root: &std::path::Path) -> PathBuf {
    data_root.join("categories.json")
}

async fn load_categories(data_root: &std::path::Path) -> CategoryStore {
    match tokio::fs::read(categories_path(data_root)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => CategoryStore::default(),
    }
}

async fn save_categories(data_root: &std::path::Path, store: &CategoryStore) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(store).expect("CategoryStore serialization is infallible");
    tokio::fs::write(categories_path(data_root), bytes).await
}

#[derive(Deserialize)]
struct CreateCategoryRequest {
    name: String,
    slug: String,
}

/// `GET /api/categories` — カテゴリ一覧(管理者のみ)。
#[handler]
async fn list_categories(req: &Request, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let store = load_categories(&state.data_root).await;
    Ok(Response::builder()
        .status(poem::http::StatusCode::OK)
        .content_type("application/json")
        .body(serde_json::to_vec(&store.categories).unwrap_or_default()))
}

/// `POST /api/categories` — カテゴリを新規作成する(管理者のみ)。
#[handler]
async fn create_category(req: &Request, state: Data<&AppState>, body: poem::web::Json<CreateCategoryRequest>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    if body.name.trim().is_empty() || body.slug.trim().is_empty() {
        return Ok(Response::builder().status(poem::http::StatusCode::BAD_REQUEST).body("name and slug must not be empty"));
    }
    let mut store = load_categories(&state.data_root).await;
    let id = store.next_id;
    store.next_id += 1;
    let category = Category { id, name: body.name.clone(), slug: body.slug.clone() };
    store.categories.push(category.clone());
    save_categories(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder()
        .status(poem::http::StatusCode::CREATED)
        .content_type("application/json")
        .body(serde_json::to_vec(&category).unwrap_or_default()))
}

/// `DELETE /api/categories/:id` — カテゴリを削除する(管理者のみ)。
/// 既存投稿の`categories`参照は自動的には剥がさない(単純さ優先、
/// 次の増分での改善候補)。
#[handler]
async fn delete_category(req: &Request, PathExtractor(id): PathExtractor<u64>, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let mut store = load_categories(&state.data_root).await;
    let before = store.categories.len();
    store.categories.retain(|c| c.id != id);
    if store.categories.len() == before {
        return Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("category not found"));
    }
    save_categories(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder().status(poem::http::StatusCode::OK).body("deleted"))
}

// ---------------------------------------------------------------------
// 固定ページ(Page) — WordPressの「固定ページ」相当。投稿(Post)とは
// 別枠で、時系列のブログフィードには含まれない独立コンテンツ
// (「About」「Contact」等)。カテゴリは持たない。`slug`は一意。
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum PageStatus {
    Draft,
    Published,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Page {
    id: u64,
    title: String,
    slug: String,
    body: String,
    status: PageStatus,
    created_at: u64,
    updated_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PageStore {
    next_id: u64,
    pages: Vec<Page>,
}

fn pages_path(data_root: &std::path::Path) -> PathBuf {
    data_root.join("pages.json")
}

async fn load_pages(data_root: &std::path::Path) -> PageStore {
    match tokio::fs::read(pages_path(data_root)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => PageStore::default(),
    }
}

async fn save_pages(data_root: &std::path::Path, store: &PageStore) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(store).expect("PageStore serialization is infallible");
    tokio::fs::write(pages_path(data_root), bytes).await
}

#[derive(Deserialize)]
struct CreatePageRequest {
    title: String,
    slug: String,
    body: String,
}

/// `POST /api/pages` — 固定ページを新規作成する(管理者のみ、
/// 初期状態は`draft`)。`slug`は既存ページと重複してはならない。
#[handler]
async fn create_page(req: &Request, state: Data<&AppState>, body: poem::web::Json<CreatePageRequest>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    if body.title.trim().is_empty() || body.slug.trim().is_empty() {
        return Ok(Response::builder().status(poem::http::StatusCode::BAD_REQUEST).body("title and slug must not be empty"));
    }
    let mut store = load_pages(&state.data_root).await;
    if store.pages.iter().any(|p| p.slug == body.slug) {
        return Ok(Response::builder().status(poem::http::StatusCode::CONFLICT).body("slug already in use"));
    }
    let id = store.next_id;
    store.next_id += 1;
    let now = now_unix();
    let page = Page {
        id,
        title: body.title.clone(),
        slug: body.slug.clone(),
        body: body.body.clone(),
        status: PageStatus::Draft,
        created_at: now,
        updated_at: now,
    };
    store.pages.push(page.clone());
    save_pages(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder()
        .status(poem::http::StatusCode::CREATED)
        .content_type("application/json")
        .body(serde_json::to_vec(&page).unwrap_or_default()))
}

/// `GET /api/pages` — 固定ページ一覧(管理者のみ)。
#[handler]
async fn list_pages(req: &Request, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let store = load_pages(&state.data_root).await;
    Ok(Response::builder()
        .status(poem::http::StatusCode::OK)
        .content_type("application/json")
        .body(serde_json::to_vec(&store.pages).unwrap_or_default()))
}

#[handler]
async fn get_page(req: &Request, PathExtractor(id): PathExtractor<u64>, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let store = load_pages(&state.data_root).await;
    match store.pages.iter().find(|p| p.id == id) {
        Some(page) => Ok(Response::builder()
            .status(poem::http::StatusCode::OK)
            .content_type("application/json")
            .body(serde_json::to_vec(page).unwrap_or_default())),
        None => Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("page not found")),
    }
}

/// `GET /api/pages/by-slug/:slug` — スラッグでの公開固定ページ取得
/// (未ログインで可、`published`状態のもののみ返す——実サイトで
/// URLスラッグから固定ページを描画する際のルックアップ相当)。
#[handler]
async fn get_page_by_slug(PathExtractor(slug): PathExtractor<String>, state: Data<&AppState>) -> PoemResult<Response> {
    let store = load_pages(&state.data_root).await;
    match store.pages.iter().find(|p| p.slug == slug && p.status == PageStatus::Published) {
        Some(page) => Ok(Response::builder()
            .status(poem::http::StatusCode::OK)
            .content_type("application/json")
            .body(serde_json::to_vec(page).unwrap_or_default())),
        None => Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("page not found")),
    }
}

#[derive(Deserialize)]
struct UpdatePageRequest {
    title: Option<String>,
    slug: Option<String>,
    body: Option<String>,
    status: Option<PageStatus>,
}

/// `PUT /api/pages/:id` — 固定ページを更新する(管理者のみ、指定した
/// フィールドのみ更新)。`slug`変更時も一意性を検証する。
#[handler]
async fn update_page(
    req: &Request,
    PathExtractor(id): PathExtractor<u64>,
    state: Data<&AppState>,
    body: poem::web::Json<UpdatePageRequest>,
) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let mut store = load_pages(&state.data_root).await;
    if let Some(slug) = &body.slug {
        if store.pages.iter().any(|p| p.id != id && &p.slug == slug) {
            return Ok(Response::builder().status(poem::http::StatusCode::CONFLICT).body("slug already in use"));
        }
    }
    let Some(page) = store.pages.iter_mut().find(|p| p.id == id) else {
        return Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("page not found"));
    };
    if let Some(title) = &body.title {
        page.title = title.clone();
    }
    if let Some(slug) = &body.slug {
        page.slug = slug.clone();
    }
    if let Some(page_body) = &body.body {
        page.body = page_body.clone();
    }
    if let Some(status) = &body.status {
        page.status = status.clone();
    }
    page.updated_at = now_unix();
    let updated = page.clone();
    save_pages(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder()
        .status(poem::http::StatusCode::OK)
        .content_type("application/json")
        .body(serde_json::to_vec(&updated).unwrap_or_default()))
}

/// `DELETE /api/pages/:id` — 固定ページを削除する(管理者のみ)。
#[handler]
async fn delete_page(req: &Request, PathExtractor(id): PathExtractor<u64>, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let mut store = load_pages(&state.data_root).await;
    let before = store.pages.len();
    store.pages.retain(|p| p.id != id);
    if store.pages.len() == before {
        return Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("page not found"));
    }
    save_pages(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder().status(poem::http::StatusCode::OK).body("deleted"))
}

// ---------------------------------------------------------------------
// タグ(Tag) — カテゴリ(Category、階層構造)とは別の、フラットな
// タグ付け機構。単純なCRUD、管理者のみ。投稿は`tags: Vec<u64>`で
// タグIDを参照する(多対多)。
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Tag {
    id: u64,
    name: String,
    slug: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TagStore {
    next_id: u64,
    tags: Vec<Tag>,
}

fn tags_path(data_root: &std::path::Path) -> PathBuf {
    data_root.join("tags.json")
}

async fn load_tags(data_root: &std::path::Path) -> TagStore {
    match tokio::fs::read(tags_path(data_root)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => TagStore::default(),
    }
}

async fn save_tags(data_root: &std::path::Path, store: &TagStore) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(store).expect("TagStore serialization is infallible");
    tokio::fs::write(tags_path(data_root), bytes).await
}

#[derive(Deserialize)]
struct CreateTagRequest {
    name: String,
    slug: String,
}

/// `GET /api/tags` — タグ一覧(管理者のみ)。
#[handler]
async fn list_tags(req: &Request, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let store = load_tags(&state.data_root).await;
    Ok(Response::builder()
        .status(poem::http::StatusCode::OK)
        .content_type("application/json")
        .body(serde_json::to_vec(&store.tags).unwrap_or_default()))
}

/// `POST /api/tags` — タグを新規作成する(管理者のみ)。
#[handler]
async fn create_tag(req: &Request, state: Data<&AppState>, body: poem::web::Json<CreateTagRequest>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    if body.name.trim().is_empty() || body.slug.trim().is_empty() {
        return Ok(Response::builder().status(poem::http::StatusCode::BAD_REQUEST).body("name and slug must not be empty"));
    }
    let mut store = load_tags(&state.data_root).await;
    let id = store.next_id;
    store.next_id += 1;
    let tag = Tag { id, name: body.name.clone(), slug: body.slug.clone() };
    store.tags.push(tag.clone());
    save_tags(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder()
        .status(poem::http::StatusCode::CREATED)
        .content_type("application/json")
        .body(serde_json::to_vec(&tag).unwrap_or_default()))
}

/// `DELETE /api/tags/:id` — タグを削除する(管理者のみ)。既存投稿の
/// `tags`参照は自動的には剥がさない(カテゴリと同じ単純さ優先の方針)。
#[handler]
async fn delete_tag(req: &Request, PathExtractor(id): PathExtractor<u64>, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let mut store = load_tags(&state.data_root).await;
    let before = store.tags.len();
    store.tags.retain(|t| t.id != id);
    if store.tags.len() == before {
        return Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("tag not found"));
    }
    save_tags(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder().status(poem::http::StatusCode::OK).body("deleted"))
}

// ---------------------------------------------------------------------
// コメント(Comment) — 投稿への未ログイン投稿を許可するが、WordPressの
// モデレーションキューと同じく初期状態は`approved: false`。
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Comment {
    id: u64,
    post_id: u64,
    author_name: String,
    body: String,
    created_at: u64,
    approved: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CommentStore {
    next_id: u64,
    comments: Vec<Comment>,
}

fn comments_path(data_root: &std::path::Path) -> PathBuf {
    data_root.join("comments.json")
}

async fn load_comments(data_root: &std::path::Path) -> CommentStore {
    match tokio::fs::read(comments_path(data_root)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => CommentStore::default(),
    }
}

async fn save_comments(data_root: &std::path::Path, store: &CommentStore) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(store).expect("CommentStore serialization is infallible");
    tokio::fs::write(comments_path(data_root), bytes).await
}

#[derive(Deserialize)]
struct CreateCommentRequest {
    author_name: String,
    body: String,
}

/// `POST /api/posts/:id/comments` — コメントを投稿する(未ログインで可、
/// ただし常に`approved: false`で作成される——WordPressのモデレーション
/// キューのデフォルト挙動と同じ)。対象の投稿が存在しない場合は404。
#[handler]
async fn create_comment(
    PathExtractor(post_id): PathExtractor<u64>,
    state: Data<&AppState>,
    body: poem::web::Json<CreateCommentRequest>,
) -> PoemResult<Response> {
    if body.author_name.trim().is_empty() || body.body.trim().is_empty() {
        return Ok(Response::builder().status(poem::http::StatusCode::BAD_REQUEST).body("author_name and body must not be empty"));
    }
    let posts = load_posts(&state.data_root).await;
    if !posts.posts.iter().any(|p| p.id == post_id) {
        return Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("post not found"));
    }
    let mut store = load_comments(&state.data_root).await;
    let id = store.next_id;
    store.next_id += 1;
    let comment = Comment {
        id,
        post_id,
        author_name: body.author_name.clone(),
        body: body.body.clone(),
        created_at: now_unix(),
        approved: false,
    };
    store.comments.push(comment.clone());
    save_comments(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder()
        .status(poem::http::StatusCode::CREATED)
        .content_type("application/json")
        .body(serde_json::to_vec(&comment).unwrap_or_default()))
}

/// `GET /api/posts/:id/comments?approved_only=true` — 指定投稿のコメント
/// 一覧(公開、未ログインで可)。`approved_only=true`を付けない場合も
/// 公開エンドポイントとしては承認済みのみ返す(未承認コメントの公開
/// 閲覧は`GET /api/comments`(管理者専用)からのみ可能)。
#[handler]
async fn list_post_comments(PathExtractor(post_id): PathExtractor<u64>, state: Data<&AppState>) -> PoemResult<Response> {
    let store = load_comments(&state.data_root).await;
    let comments: Vec<&Comment> = store.comments.iter().filter(|c| c.post_id == post_id && c.approved).collect();
    Ok(Response::builder()
        .status(poem::http::StatusCode::OK)
        .content_type("application/json")
        .body(serde_json::to_vec(&comments).unwrap_or_default()))
}

/// `GET /api/comments` — 全コメント一覧(管理者のみ、未承認も含む)。
#[handler]
async fn list_all_comments(req: &Request, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let store = load_comments(&state.data_root).await;
    Ok(Response::builder()
        .status(poem::http::StatusCode::OK)
        .content_type("application/json")
        .body(serde_json::to_vec(&store.comments).unwrap_or_default()))
}

/// `POST /api/comments/:id/approve` — コメントを承認する(管理者のみ)。
#[handler]
async fn approve_comment(req: &Request, PathExtractor(id): PathExtractor<u64>, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let mut store = load_comments(&state.data_root).await;
    let Some(comment) = store.comments.iter_mut().find(|c| c.id == id) else {
        return Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("comment not found"));
    };
    comment.approved = true;
    let updated = comment.clone();
    save_comments(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder()
        .status(poem::http::StatusCode::OK)
        .content_type("application/json")
        .body(serde_json::to_vec(&updated).unwrap_or_default()))
}

/// `DELETE /api/comments/:id` — コメントを削除する(管理者のみ)。
#[handler]
async fn delete_comment(req: &Request, PathExtractor(id): PathExtractor<u64>, state: Data<&AppState>) -> PoemResult<Response> {
    require_admin_session(req, &state)?;
    let mut store = load_comments(&state.data_root).await;
    let before = store.comments.len();
    store.comments.retain(|c| c.id != id);
    if store.comments.len() == before {
        return Ok(Response::builder().status(poem::http::StatusCode::NOT_FOUND).body("comment not found"));
    }
    save_comments(&state.data_root, &store)
        .await
        .map_err(|e| poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder().status(poem::http::StatusCode::OK).body("deleted"))
}

/// トップページ(`GET /`)のHTMLランディングページ。
/// ブラウザで実インスタンスへアクセスしたユーザーへ、アプリの概要・
/// 実装済みAPI一覧・未実装機能の正直な開示・ダウンロードリンクを示す
/// (JSON APIのみで何も表示されないUXバグの修正)。
const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>RS-Blog</title>
<style>
  body { font-family: system-ui, sans-serif; max-width: 780px; margin: 2rem auto; padding: 0 1rem; line-height: 1.6; color: #222; }
  h1 { margin-bottom: 0; }
  .tagline { color: #666; margin-top: 0.2rem; }
  code { background: #f2f2f2; padding: 0.1rem 0.35rem; border-radius: 3px; }
  table { border-collapse: collapse; width: 100%; margin: 1rem 0; }
  th, td { text-align: left; padding: 0.4rem 0.6rem; border-bottom: 1px solid #ddd; font-size: 0.92rem; }
  .warn { background: #fff8e1; border: 1px solid #ffe08a; border-radius: 6px; padding: 0.8rem 1rem; }
  .btn { display: inline-block; background: #2d6cdf; color: #fff; padding: 0.5rem 1rem; border-radius: 6px; text-decoration: none; margin-right: 0.5rem; }
  footer { color: #888; font-size: 0.85rem; margin-top: 2rem; }
</style>
</head>
<body>
<h1>RS-Blog</h1>
<p class="tagline">WordPress相当のブログエンジン — Rust + poem(RPoem)製、高速・高セキュリティ・省メモリ志向。v0.1.0。</p>

<h2>これは何?</h2>
<p>
  <a href="https://wordpress.org/">WordPress</a>のRust版を目指すプロジェクトです。
  v0.1.0時点では投稿(Post)のCRUDとOTPログイン(管理者のみ)を実装しています。
</p>

<h2>使い方: 現在はJSON APIのみ(ブラウザUIはまだありません)</h2>
<p>このページ以外はすべてJSON APIです。以下のエンドポイントに対して<code>curl</code>や外部クライアントからアクセスしてください。</p>
<table>
<tr><th>メソッド / パス</th><th>説明</th></tr>
<tr><td><code>GET /healthz</code></td><td>ヘルスチェック</td></tr>
<tr><td><code>POST /api/auth/request-otp</code></td><td>ログイン用ワンタイムパスワードをメール送信(管理者のみ)</td></tr>
<tr><td><code>POST /api/auth/verify-otp</code></td><td>OTPを検証してセッショントークンを発行</td></tr>
<tr><td><code>POST /api/auth/logout</code></td><td>ログアウト(トークン失効)</td></tr>
<tr><td><code>GET /api/posts</code> / <code>POST /api/posts</code></td><td>投稿一覧取得(ログイン必須) / 新規作成</td></tr>
<tr><td><code>GET /api/posts/:id</code></td><td>投稿詳細取得</td></tr>
<tr><td><code>PUT /api/posts/:id</code></td><td>投稿更新(ステータス変更含む、<code>draft</code>/<code>published</code>)</td></tr>
<tr><td><code>DELETE /api/posts/:id</code></td><td>投稿削除</td></tr>
<tr><td><code>GET /api/posts?category=:id</code></td><td>カテゴリIDで投稿を絞り込み一覧</td></tr>
<tr><td><code>GET /api/posts?tag=:id</code></td><td>タグIDで投稿を絞り込み一覧</td></tr>
<tr><td><code>GET /api/categories</code> / <code>POST /api/categories</code></td><td>カテゴリ一覧取得(ログイン必須) / 新規作成</td></tr>
<tr><td><code>DELETE /api/categories/:id</code></td><td>カテゴリ削除</td></tr>
<tr><td><code>GET /api/tags</code> / <code>POST /api/tags</code></td><td>タグ一覧取得(ログイン必須) / 新規作成</td></tr>
<tr><td><code>DELETE /api/tags/:id</code></td><td>タグ削除</td></tr>
<tr><td><code>GET /api/pages</code> / <code>POST /api/pages</code></td><td>固定ページ一覧取得(ログイン必須) / 新規作成</td></tr>
<tr><td><code>GET /api/pages/:id</code></td><td>固定ページ詳細取得(ログイン必須)</td></tr>
<tr><td><code>PUT /api/pages/:id</code></td><td>固定ページ更新(<code>slug</code>は一意性検証あり)</td></tr>
<tr><td><code>DELETE /api/pages/:id</code></td><td>固定ページ削除</td></tr>
<tr><td><code>GET /api/pages/by-slug/:slug</code></td><td>スラッグで公開固定ページ取得(未ログイン可、<code>published</code>のみ)</td></tr>
<tr><td><code>POST /api/posts/:id/comments</code></td><td>コメント投稿(未ログイン可、常に未承認状態で作成)</td></tr>
<tr><td><code>GET /api/posts/:id/comments?approved_only=true</code></td><td>指定投稿の承認済みコメント一覧(公開)</td></tr>
<tr><td><code>GET /api/comments</code></td><td>全コメント一覧(未承認含む、ログイン必須)</td></tr>
<tr><td><code>POST /api/comments/:id/approve</code></td><td>コメントを承認</td></tr>
<tr><td><code>DELETE /api/comments/:id</code></td><td>コメント削除</td></tr>
</table>

<div class="warn">
<strong>正直な開示: まだ実装していない機能</strong>
<ul>
<li>カスタム投稿タイプ</li>
<li>テーマ・ウィジェット</li>
<li>プラグイン機構(PHPプラグイン互換レイヤは技術調査段階、未着手)</li>
<li>メディアライブラリ</li>
<li>ユーザー・ロール・権限管理(登録アカウント制・アクセス制御の細分化)</li>
<li><code>aruaru-db</code>/PostgreSQL DUAL DB構成(現状はJSONファイル永続化のみ)</li>
</ul>
</div>

<h2>ダウンロード / インストール</h2>
<p>
  <a class="btn" href="https://github.com/aon-co-jp/RS-Blog/releases/latest">最新リリースをダウンロード</a>
  <a class="btn" href="https://github.com/aon-co-jp/RS-Blog">GitHubでソースを見る</a>
</p>
<p>Linux(静的リンクmuslバイナリ)・Windows向けにインストーラー付きビルド済みバイナリを配布しています。詳細は<a href="https://github.com/aon-co-jp/RS-Blog#readme">README</a>参照。</p>

<footer>RS-Blog v0.1.0 &mdash; <a href="https://github.com/aon-co-jp/RS-Blog">aon-co-jp/RS-Blog</a></footer>
</body>
</html>
"#;

#[handler]
async fn index() -> Response {
    Response::builder()
        .status(poem::http::StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(INDEX_HTML)
}

#[handler]
async fn healthz() -> &'static str {
    "ok"
}

#[handler]
async fn request_otp(state: Data<&AppState>, body: poem::web::Json<serde_json::Value>) -> PoemResult<Response> {
    let email = body.get("email").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if email != state.admin_email {
        return Ok(Response::builder().status(poem::http::StatusCode::FORBIDDEN).body("email not registered"));
    }
    let Some(smtp) = state.smtp.clone() else {
        return Ok(Response::builder().status(poem::http::StatusCode::SERVICE_UNAVAILABLE).body("SMTP not configured"));
    };
    let auth::RequestOtpOutcome::Issued(code) = state.auth.request_otp(&email);
    match mail::send_otp(smtp, email, code).await {
        Ok(()) => Ok(Response::builder().status(poem::http::StatusCode::OK).body("otp sent")),
        Err(e) => {
            tracing::warn!("failed to send OTP mail: {e}");
            Ok(Response::builder().status(poem::http::StatusCode::BAD_GATEWAY).body("failed to send mail"))
        }
    }
}

#[derive(Deserialize)]
struct VerifyOtpRequest {
    email: String,
    code: String,
}

#[handler]
async fn verify_otp(state: Data<&AppState>, body: poem::web::Json<VerifyOtpRequest>) -> PoemResult<Response> {
    match state.auth.consume_otp(&body.email, &body.code) {
        Ok(()) => {
            let token = state.auth.create_session(&body.email);
            Ok(Response::builder()
                .status(poem::http::StatusCode::OK)
                .content_type("application/json")
                .body(serde_json::to_vec(&serde_json::json!({ "token": token })).unwrap_or_default()))
        }
        Err(e) => Ok(Response::builder().status(poem::http::StatusCode::FORBIDDEN).body(e.message())),
    }
}

/// `POST /api/auth/logout` — セッショントークンを失効させる。
#[handler]
async fn logout(req: &Request, state: Data<&AppState>) -> PoemResult<Response> {
    let header = req.header(poem::http::header::AUTHORIZATION).unwrap_or("");
    if let Some(token) = header.strip_prefix("Bearer ") {
        state.auth.logout(token);
    }
    Ok(Response::builder().status(poem::http::StatusCode::OK).body("logged out"))
}

fn env_data_dir() -> PathBuf {
    std::env::var("RSBLOG_DATA_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("./data"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let data_root = env_data_dir();
    tokio::fs::create_dir_all(&data_root).await?;
    tracing::info!("rs-blog v0.1.0 starting, data_root={:?}", data_root);

    let admin_email = std::env::var("RSBLOG_ADMIN_EMAIL").unwrap_or_else(|_| "admin@example.com".to_string());
    let smtp = mail::SmtpConfig::from_env();
    if smtp.is_none() {
        tracing::warn!("RSBLOG_SMTP_* not fully configured; /api/auth/request-otp will return 503");
    }
    let state = AppState { data_root, auth: Arc::new(auth::AuthStore::default()), admin_email, smtp };

    let app = Route::new()
        .at("/", get(index))
        .at("/healthz", get(healthz))
        .at("/api/auth/request-otp", post(request_otp))
        .at("/api/auth/verify-otp", post(verify_otp))
        .at("/api/auth/logout", post(logout))
        .at("/api/posts", get(list_posts).post(create_post))
        .at("/api/posts/:id", get(get_post).put(update_post).delete(delete_post))
        .at("/api/categories", get(list_categories).post(create_category))
        .at("/api/categories/:id", delete(delete_category))
        .at("/api/pages", get(list_pages).post(create_page))
        .at("/api/pages/by-slug/:slug", get(get_page_by_slug))
        .at("/api/pages/:id", get(get_page).put(update_page).delete(delete_page))
        .at("/api/tags", get(list_tags).post(create_tag))
        .at("/api/tags/:id", delete(delete_tag))
        .at("/api/posts/:id/comments", get(list_post_comments).post(create_comment))
        .at("/api/comments", get(list_all_comments))
        .at("/api/comments/:id/approve", post(approve_comment))
        .at("/api/comments/:id", delete(delete_comment))
        .data(state)
        .with(Tracing);

    let port = std::env::var("RSBLOG_PORT").unwrap_or_else(|_| "8101".to_string());
    let addr = format!("0.0.0.0:{port}");
    tracing::info!("listening on {addr}");
    Server::new(TcpListener::bind(addr)).run(app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use poem::test::TestClient;

    fn test_state(dir: &std::path::Path) -> AppState {
        AppState { data_root: dir.to_path_buf(), auth: Arc::new(auth::AuthStore::default()), admin_email: "admin@example.com".to_string(), smtp: None }
    }

    fn app_for(state: AppState) -> impl poem::Endpoint {
        Route::new()
            .at("/", get(index))
            .at("/healthz", get(healthz))
            .at("/api/auth/request-otp", post(request_otp))
            .at("/api/auth/verify-otp", post(verify_otp))
            .at("/api/auth/logout", post(logout))
            .at("/api/posts", get(list_posts).post(create_post))
            .at("/api/posts/:id", get(get_post).put(update_post).delete(delete_post))
            .at("/api/categories", get(list_categories).post(create_category))
            .at("/api/categories/:id", delete(delete_category))
            .at("/api/pages", get(list_pages).post(create_page))
            .at("/api/pages/by-slug/:slug", get(get_page_by_slug))
            .at("/api/pages/:id", get(get_page).put(update_page).delete(delete_page))
            .at("/api/tags", get(list_tags).post(create_tag))
            .at("/api/tags/:id", delete(delete_tag))
            .at("/api/posts/:id/comments", get(list_post_comments).post(create_comment))
            .at("/api/comments", get(list_all_comments))
            .at("/api/comments/:id/approve", post(approve_comment))
            .at("/api/comments/:id", delete(delete_comment))
            .data(state)
    }

    async fn admin_token(state: &AppState) -> String {
        let auth::RequestOtpOutcome::Issued(code) = state.auth.request_otp(&state.admin_email);
        state.auth.consume_otp(&state.admin_email, &code).unwrap();
        state.auth.create_session(&state.admin_email)
    }

    #[tokio::test]
    async fn root_returns_landing_page_with_key_markers() {
        // UXバグ修正の検証: JSON APIオンリーで何も表示されなかった`GET /`が
        // アプリ名・実エンドポイント・ダウンロードリンクを含むHTMLを返すこと。
        let dir = tempdir();
        let state = test_state(dir.path());
        let client = TestClient::new(app_for(state));
        let resp = client.get("/").send().await;
        resp.assert_status_is_ok();
        let body = resp.0.into_body().into_string().await.unwrap();
        assert!(body.contains("RS-Blog"));
        assert!(body.contains("/api/posts"));
        assert!(body.contains("https://github.com/aon-co-jp/RS-Blog/releases/latest"));
    }

    #[tokio::test]
    async fn listing_posts_without_auth_is_rejected() {
        let dir = tempdir();
        let state = test_state(dir.path());
        let client = TestClient::new(app_for(state));
        let resp = client.get("/api/posts").send().await;
        resp.assert_status(poem::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_then_list_then_get_post_roundtrips() {
        let dir = tempdir();
        let state = test_state(dir.path());
        let token = admin_token(&state).await;
        let client = TestClient::new(app_for(state));

        let resp = client
            .post("/api/posts")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"title": "Hello", "body": "World"}))
            .send()
            .await;
        resp.assert_status(poem::http::StatusCode::CREATED);
        let created: Post = resp.json().await.value().deserialize();
        assert_eq!(created.status, PostStatus::Draft);

        let resp = client.get("/api/posts").header("Authorization", format!("Bearer {token}")).send().await;
        resp.assert_status_is_ok();
        let posts: Vec<Post> = resp.json().await.value().deserialize();
        assert_eq!(posts.len(), 1);

        let resp = client.get(format!("/api/posts/{}", created.id)).header("Authorization", format!("Bearer {token}")).send().await;
        resp.assert_status_is_ok();
    }

    #[tokio::test]
    async fn updating_a_post_changes_status_and_fields() {
        let dir = tempdir();
        let state = test_state(dir.path());
        let token = admin_token(&state).await;
        let client = TestClient::new(app_for(state));

        let resp = client
            .post("/api/posts")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"title": "Draft post", "body": "..."}))
            .send()
            .await;
        let created: Post = resp.json().await.value().deserialize();

        let resp = client
            .put(format!("/api/posts/{}", created.id))
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"status": "published"}))
            .send()
            .await;
        resp.assert_status_is_ok();
        let updated: Post = resp.json().await.value().deserialize();
        assert_eq!(updated.status, PostStatus::Published);
    }

    #[tokio::test]
    async fn deleting_a_post_removes_it() {
        let dir = tempdir();
        let state = test_state(dir.path());
        let token = admin_token(&state).await;
        let client = TestClient::new(app_for(state));

        let resp = client
            .post("/api/posts")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"title": "To delete", "body": "..."}))
            .send()
            .await;
        let created: Post = resp.json().await.value().deserialize();

        let resp = client.delete(format!("/api/posts/{}", created.id)).header("Authorization", format!("Bearer {token}")).send().await;
        resp.assert_status_is_ok();

        let resp = client.get(format!("/api/posts/{}", created.id)).header("Authorization", format!("Bearer {token}")).send().await;
        resp.assert_status(poem::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn creating_a_post_with_empty_title_is_rejected() {
        let dir = tempdir();
        let state = test_state(dir.path());
        let token = admin_token(&state).await;
        let client = TestClient::new(app_for(state));

        let resp = client
            .post("/api/posts")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"title": "  ", "body": "..."}))
            .send()
            .await;
        resp.assert_status(poem::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn category_crud_and_post_filtering_by_category() {
        let dir = tempdir();
        let state = test_state(dir.path());
        let token = admin_token(&state).await;
        let client = TestClient::new(app_for(state));

        let resp = client
            .post("/api/categories")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"name": "News", "slug": "news"}))
            .send()
            .await;
        resp.assert_status(poem::http::StatusCode::CREATED);
        let category: Category = resp.json().await.value().deserialize();

        let resp = client
            .post("/api/posts")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"title": "Categorized", "body": "...", "categories": [category.id]}))
            .send()
            .await;
        resp.assert_status(poem::http::StatusCode::CREATED);

        let resp = client
            .post("/api/posts")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"title": "Uncategorized", "body": "..."}))
            .send()
            .await;
        resp.assert_status(poem::http::StatusCode::CREATED);

        let resp = client
            .get(format!("/api/posts?category={}", category.id))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await;
        resp.assert_status_is_ok();
        let posts: Vec<Post> = resp.json().await.value().deserialize();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].title, "Categorized");

        let resp = client
            .delete(format!("/api/categories/{}", category.id))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await;
        resp.assert_status_is_ok();
    }

    #[tokio::test]
    async fn comment_moderation_queue_hides_unapproved_until_admin_approves() {
        let dir = tempdir();
        let state = test_state(dir.path());
        let token = admin_token(&state).await;
        let client = TestClient::new(app_for(state));

        let resp = client
            .post("/api/posts")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"title": "With comments", "body": "..."}))
            .send()
            .await;
        let post: Post = resp.json().await.value().deserialize();

        // 未ログインでコメント投稿できる(常にapproved: falseで作成される)。
        let resp = client
            .post(format!("/api/posts/{}/comments", post.id))
            .body_json(&serde_json::json!({"author_name": "Alice", "body": "Nice post!"}))
            .send()
            .await;
        resp.assert_status(poem::http::StatusCode::CREATED);
        let comment: Comment = resp.json().await.value().deserialize();
        assert!(!comment.approved);

        // 承認前は公開一覧(approved_only)に出てこない。
        let resp = client.get(format!("/api/posts/{}/comments?approved_only=true", post.id)).send().await;
        resp.assert_status_is_ok();
        let visible: Vec<Comment> = resp.json().await.value().deserialize();
        assert!(visible.is_empty());

        // 管理者は全件(未承認含む)を見られる。
        let resp = client.get("/api/comments").header("Authorization", format!("Bearer {token}")).send().await;
        resp.assert_status_is_ok();
        let all: Vec<Comment> = resp.json().await.value().deserialize();
        assert_eq!(all.len(), 1);

        // 管理者が承認すると公開一覧に出るようになる。
        let resp = client
            .post(format!("/api/comments/{}/approve", comment.id))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await;
        resp.assert_status_is_ok();

        let resp = client.get(format!("/api/posts/{}/comments?approved_only=true", post.id)).send().await;
        resp.assert_status_is_ok();
        let visible: Vec<Comment> = resp.json().await.value().deserialize();
        assert_eq!(visible.len(), 1);
    }

    #[tokio::test]
    async fn page_crud_slug_uniqueness_and_by_slug_lookup() {
        let dir = tempdir();
        let state = test_state(dir.path());
        let token = admin_token(&state).await;
        let client = TestClient::new(app_for(state));

        let resp = client
            .post("/api/pages")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"title": "About", "slug": "about", "body": "We are..."}))
            .send()
            .await;
        resp.assert_status(poem::http::StatusCode::CREATED);
        let page: Page = resp.json().await.value().deserialize();
        assert_eq!(page.status, PageStatus::Draft);

        // 同一slugでの重複作成は409。
        let resp = client
            .post("/api/pages")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"title": "About Again", "slug": "about", "body": "..."}))
            .send()
            .await;
        resp.assert_status(poem::http::StatusCode::CONFLICT);

        // draft状態ではby-slugルックアップ(公開用)で404。
        let resp = client.get("/api/pages/by-slug/about").send().await;
        resp.assert_status(poem::http::StatusCode::NOT_FOUND);

        // publishedにすると公開ルックアップで見えるようになる。
        let resp = client
            .put(format!("/api/pages/{}", page.id))
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"status": "published"}))
            .send()
            .await;
        resp.assert_status_is_ok();

        let resp = client.get("/api/pages/by-slug/about").send().await;
        resp.assert_status_is_ok();
        let found: Page = resp.json().await.value().deserialize();
        assert_eq!(found.title, "About");

        let resp = client.get("/api/pages").header("Authorization", format!("Bearer {token}")).send().await;
        resp.assert_status_is_ok();
        let pages: Vec<Page> = resp.json().await.value().deserialize();
        assert_eq!(pages.len(), 1);

        let resp = client.delete(format!("/api/pages/{}", page.id)).header("Authorization", format!("Bearer {token}")).send().await;
        resp.assert_status_is_ok();

        let resp = client.get("/api/pages/by-slug/about").send().await;
        resp.assert_status(poem::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn tag_crud_and_post_filtering_by_tag() {
        let dir = tempdir();
        let state = test_state(dir.path());
        let token = admin_token(&state).await;
        let client = TestClient::new(app_for(state));

        let resp = client
            .post("/api/tags")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"name": "Rust", "slug": "rust"}))
            .send()
            .await;
        resp.assert_status(poem::http::StatusCode::CREATED);
        let tag: Tag = resp.json().await.value().deserialize();

        let resp = client
            .post("/api/posts")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"title": "Tagged", "body": "...", "tags": [tag.id]}))
            .send()
            .await;
        resp.assert_status(poem::http::StatusCode::CREATED);

        let resp = client
            .post("/api/posts")
            .header("Authorization", format!("Bearer {token}"))
            .body_json(&serde_json::json!({"title": "Untagged", "body": "..."}))
            .send()
            .await;
        resp.assert_status(poem::http::StatusCode::CREATED);

        let resp = client
            .get(format!("/api/posts?tag={}", tag.id))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await;
        resp.assert_status_is_ok();
        let posts: Vec<Post> = resp.json().await.value().deserialize();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].title, "Tagged");

        let resp = client.delete(format!("/api/tags/{}", tag.id)).header("Authorization", format!("Bearer {token}")).send().await;
        resp.assert_status_is_ok();
    }

    fn tempdir() -> TempDir {
        TempDir::new()
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("rs-blog-test-{}", generate_suffix()));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn generate_suffix() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 8] = rng.gen();
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
