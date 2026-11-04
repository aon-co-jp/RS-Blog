# RS-Blog

**開発開始日: 2026-07-21**(このリポジトリのGitHub作成日)

[WordPress](https://wordpress.org/)のRust＋[RPoem](https://github.com/aon-co-jp/RPoem)版です。開発を開始。
ハイスピードでハイセキュリティで省メモリなどの新規の特徴を持ちます。
まだ開発途中ですが、運用時は、VPSレンタルサーバー費用を安く抑えられる予定で御座います。

## 現状(v0.1.0)

> ⚠️ **正直な開示**: v0.1.0時点では投稿(Post)のCRUD+OTPログイン(管理者のみ)+
> カテゴリ/タグ/コメント/固定ページ機能を実装している。WordPressが持つ以下の機能は**まだ一切無い**:
>
> - カスタム投稿タイプ
> - テーマ・ウィジェット
> - プラグイン機構(PHPプラグイン互換レイヤは技術調査段階、未着手)
> - メディアライブラリ
> - ユーザー・ロール・権限管理(登録アカウント制・アクセス制御の細分化)
> - `aruaru-db`/PostgreSQL DUAL DB構成(現状はJSONファイル永続化のみ)

実装済みのAPI:

- `GET /healthz` — ヘルスチェック
- `POST /api/auth/request-otp` / `POST /api/auth/verify-otp` / `POST /api/auth/logout` — OTPメールログイン(管理者のみ、`RSBLOG_ADMIN_EMAIL`宛)
- `POST /api/posts` / `GET /api/posts` — 投稿の作成・一覧(ログイン必須、`GET /api/posts?category=:id`でカテゴリ絞り込み、`GET /api/posts?tag=:id`でタグ絞り込み可)
- `GET /api/posts/:id` / `PUT /api/posts/:id` / `DELETE /api/posts/:id` — 投稿の取得・更新・削除(ログイン必須)
- `GET /api/categories` / `POST /api/categories` — カテゴリの一覧・新規作成(ログイン必須)
- `DELETE /api/categories/:id` — カテゴリ削除(ログイン必須)
- `GET /api/tags` / `POST /api/tags` — タグ(カテゴリとは別のフラットな分類軸)の一覧・新規作成(ログイン必須)
- `DELETE /api/tags/:id` — タグ削除(ログイン必須)
- `GET /api/pages` / `POST /api/pages` — 固定ページ(投稿と別枠、ブログフィードに含まれない独立コンテンツ)の一覧・新規作成(ログイン必須)
- `GET /api/pages/:id` / `PUT /api/pages/:id` / `DELETE /api/pages/:id` — 固定ページの取得・更新・削除(ログイン必須、`slug`は一意性検証あり)
- `GET /api/pages/by-slug/:slug` — スラッグでの公開固定ページ取得(未ログイン可、`published`状態のもののみ)
- `POST /api/posts/:id/comments` — コメント投稿(未ログイン可、WordPressのモデレーションキューと同じく常に未承認状態で作成)
- `GET /api/posts/:id/comments?approved_only=true` — 指定投稿の承認済みコメント一覧(公開)
- `GET /api/comments` — 全コメント一覧(未承認含む、ログイン必須)
- `POST /api/comments/:id/approve` / `DELETE /api/comments/:id` — コメントの承認・削除(ログイン必須)

投稿は`draft`(下書き)/`published`(公開)の2ステータスに加え、`categories: Vec<u64>`
でカテゴリID、`tags: Vec<u64>`でタグIDを複数参照可能。固定ページは投稿とは別モデル
(`Page`)で、カテゴリ/タグを持たず`slug`で一意にアドレスされる。永続化はJSONファイル
(`RSBLOG_DATA_DIR/posts.json`・`categories.json`・`tags.json`・`pages.json`・
`comments.json`)。詳細な設計方針・今後の予定は`CLAUDE.md`のHANDOFFセクションを参照。

## インストール(ビルド済みバイナリ、インストーラー付き)

タグ付きリリース(`vX.Y.Z`)ごとに、GitHub Actions
(`.github/workflows/release.yml`)がLinux・Windows向けバイナリを
自動ビルドし、[GitHub Releases](https://github.com/aon-co-jp/RS-Blog/releases)へ添付する。

### Linux(AlmaLinux・Ubuntu・Debian・Fedora・RHEL等、systemdを使う主要ディストリ共通)

静的リンクされたmuslバイナリのため、ディストリ固有のライブラリ依存は無い。

```bash
curl -fsSL https://github.com/aon-co-jp/RS-Blog/releases/latest/download/rs-blog-linux-x86_64.tar.gz | tar xz
sudo ./install.sh
sudo systemctl edit rs-blog   # RSBLOG_ADMIN_EMAIL等を設定
sudo systemctl enable --now rs-blog
```

### Windows / Windows Server

管理者権限のPowerShellで:

```powershell
Invoke-WebRequest -Uri "https://github.com/aon-co-jp/RS-Blog/releases/latest/download/rs-blog-windows-x86_64.zip" -OutFile rs-blog.zip
Expand-Archive rs-blog.zip -DestinationPath rs-blog
cd rs-blog
.\install.ps1
```

## ソースからビルド

```bash
cargo build --release
```

## 環境変数

| 変数名 | 説明 | デフォルト |
| --- | --- | --- |
| `RSBLOG_DATA_DIR` | JSONデータの保存先ディレクトリ | `./data` |
| `RSBLOG_PORT` | リッスンポート | `8101` |
| `RSBLOG_ADMIN_EMAIL` | 管理者ログイン用メールアドレス | `admin@example.com` |
| `RSBLOG_SMTP_HOST` | SMTPホスト | (未設定なら`request-otp`は503) |
| `RSBLOG_SMTP_PORT` | SMTPポート | `587` |
| `RSBLOG_SMTP_USERNAME` | SMTPユーザー名 | — |
| `RSBLOG_SMTP_PASSWORD` | SMTPパスワード | — |
| `RSBLOG_SMTP_FROM` | 送信元メールアドレス | — |

## テスト

```bash
cargo test
```

v0.1.0時点で11件(OTP認証まわり6件+投稿CRUD5件)。

## ライセンス

Apache-2.0

詳細は`CLAUDE.md`(設計思想＆開発方針＆開発環境ルール)・`PORTING.md`(お引越しポーター)を参照。

## API 方針: REST 全廃・gRPC / Connect へ (2026-11-04)

### 日本語
- **REST は全廃する。新しい API は REST で作らない。** ブラウザや `curl` から呼ぶ API は **Connect の単純呼び出し**(`POST /<パッケージ>.<Service>/<Method>`、`content-type: application/json`、本文は JSON)、サービス間は **gRPC / gRPC-web**(RPoem の管理API `open_runo.admin.v1.Admin` と同じ流儀)。ストリーミングは、必要になってから。
- **既存の REST は、触るときに順に移す。** 移したら旧 REST の経路は消す。移行が終わるまで残るが、**新規には追加しない**。
- **JSON は RS-JSON に統一**する(読み込みは `from_str_secure` / `parse_secure` でサイズ・深さ・重複キーの上限つき、書き出しは `rust_json::fast`)。ブラウザの WASM に `prost` は入れない(サイズが増えるため)。
- **例外(自分たちの API ではなく、相手の規格が決めているもの)**: ACME、OAuth/OIDC のコールバック、第三者(GitHub など)からの Webhook、ヘルスチェック、静的ファイル配信、OTLP など。これらは規格どおりの HTTP を受ける。
- 参考実装: `RPoem`(`open_runo.admin.v1.Admin`)、`open-easy-web`(`server/src/connect.rs`、`proto/open_easy_web/admin/v1/admin.proto`)。
- ※ このリポジトリのコードが既に方針どおりかどうかは、各リポジトリの現状による。この節は方針の記録であり、コードの変更を意味しない。

### English
- **REST is being abolished. New APIs are not built as REST.** APIs called from browsers or `curl` use **Connect unary calls** (`POST /<package>.<Service>/<Method>`, `content-type: application/json`, JSON body); service-to-service calls use **gRPC / gRPC-web** (the same style as RPoem's admin API `open_runo.admin.v1.Admin`). Streaming only when it is actually needed.
- **Existing REST endpoints are migrated one by one, when they are touched.** The old REST route is removed once migrated. Until migration is finished they remain, but **no new ones are added**.
- **JSON is unified on RS-JSON**: reading with `from_str_secure` / `parse_secure` (size, depth and duplicate-key limits), writing with `rust_json::fast`. `prost` is not put into browser WASM (it would grow the binary).
- **Exceptions (shapes dictated by someone else's standard, not our own API)**: ACME, OAuth/OIDC callbacks, webhooks from third parties (e.g. GitHub), health checks, static file serving, OTLP, and so on. These accept plain HTTP as the standard requires.
- Reference implementations: `RPoem` (`open_runo.admin.v1.Admin`) and `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Note: whether the code in a given repository already follows this policy depends on that repository. This section records the policy; it does not mean the code has been changed.

### Deutsch (KI-Übersetzung, noch nicht von Muttersprachlern geprüft)
- **REST wird abgeschafft. Neue APIs werden nicht als REST gebaut.** APIs, die aus dem Browser oder mit `curl` aufgerufen werden, nutzen **Connect-Unary-Aufrufe** (`POST /<Paket>.<Service>/<Methode>`, `content-type: application/json`, JSON-Body); zwischen Diensten gilt **gRPC / gRPC-web** (wie bei der Admin-API von RPoem, `open_runo.admin.v1.Admin`). Streaming erst, wenn es wirklich nötig ist.
- **Bestehende REST-Endpunkte werden einzeln migriert, sobald man sie anfasst.** Nach der Migration wird die alte REST-Route entfernt. Bis dahin bleiben sie bestehen, es werden aber **keine neuen hinzugefügt**.
- **JSON wird auf RS-JSON vereinheitlicht**: Lesen mit `from_str_secure` / `parse_secure` (Grenzen für Größe, Tiefe und doppelte Schlüssel), Schreiben mit `rust_json::fast`. `prost` kommt nicht in Browser-WASM (die Datei würde größer).
- **Ausnahmen (Formate, die ein fremder Standard vorgibt, nicht unsere eigene API)**: ACME, OAuth/OIDC-Callbacks, Webhooks von Dritten (z. B. GitHub), Health-Checks, statische Dateien, OTLP usw. Diese nehmen schlichtes HTTP entgegen, wie es der Standard verlangt.
- Referenzimplementierungen: `RPoem` (`open_runo.admin.v1.Admin`) und `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Hinweis: Ob der Code eines Repositorys diese Richtlinie schon befolgt, hängt vom jeweiligen Repository ab. Dieser Abschnitt hält die Richtlinie fest; er bedeutet nicht, dass der Code bereits geändert wurde.

### Русский (машинный перевод, носителями языка ещё не проверен)
- **REST полностью отменяется. Новые API не создаются как REST.** API, вызываемые из браузера или через `curl`, используют **унарные вызовы Connect** (`POST /<пакет>.<Service>/<Method>`, `content-type: application/json`, тело — JSON); между сервисами — **gRPC / gRPC-web** (так же, как административный API RPoem, `open_runo.admin.v1.Admin`). Потоковая передача — только когда она действительно нужна.
- **Существующие REST-эндпоинты переносятся по одному, когда до них доходят руки.** После переноса старый REST-маршрут удаляется. До завершения переноса они остаются, но **новые не добавляются**.
- **JSON унифицируется на RS-JSON**: чтение через `from_str_secure` / `parse_secure` (ограничения на размер, глубину и повторяющиеся ключи), запись через `rust_json::fast`. `prost` не добавляется в браузерный WASM (размер вырос бы).
- **Исключения (форматы, заданные чужим стандартом, а не наш собственный API)**: ACME, обратные вызовы OAuth/OIDC, вебхуки сторонних сервисов (например, GitHub), проверки работоспособности, раздача статических файлов, OTLP и т. п. Они принимают обычный HTTP, как требует стандарт.
- Эталонные реализации: `RPoem` (`open_runo.admin.v1.Admin`) и `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Примечание: соответствует ли код конкретного репозитория этой политике, зависит от самого репозитория. Этот раздел фиксирует политику и не означает, что код уже изменён.
