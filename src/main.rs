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
//! - 固定ページ・カスタム投稿タイプ
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
    get, handler, post,
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
    let post = Post { id, title: body.title.clone(), body: body.body.clone(), status: PostStatus::Draft, created_at: now, updated_at: now };
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
    Ok(Response::builder()
        .status(poem::http::StatusCode::OK)
        .content_type("application/json")
        .body(serde_json::to_vec(&store.posts).unwrap_or_default()))
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
        .at("/healthz", get(healthz))
        .at("/api/auth/request-otp", post(request_otp))
        .at("/api/auth/verify-otp", post(verify_otp))
        .at("/api/auth/logout", post(logout))
        .at("/api/posts", get(list_posts).post(create_post))
        .at("/api/posts/:id", get(get_post).put(update_post).delete(delete_post))
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
            .at("/healthz", get(healthz))
            .at("/api/auth/request-otp", post(request_otp))
            .at("/api/auth/verify-otp", post(verify_otp))
            .at("/api/auth/logout", post(logout))
            .at("/api/posts", get(list_posts).post(create_post))
            .at("/api/posts/:id", get(get_post).put(update_post).delete(delete_post))
            .data(state)
    }

    async fn admin_token(state: &AppState) -> String {
        let auth::RequestOtpOutcome::Issued(code) = state.auth.request_otp(&state.admin_email);
        state.auth.consume_otp(&state.admin_email, &code).unwrap();
        state.auth.create_session(&state.admin_email)
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
