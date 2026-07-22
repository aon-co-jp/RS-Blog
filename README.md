# RS-Blog

**開発開始日: 2026-07-21**(このリポジトリのGitHub作成日)

[WordPress](https://wordpress.org/)のRust＋[RPoem](https://github.com/aon-co-jp/RPoem)版です。開発を開始。
ハイスピードでハイセキュリティで省メモリなどの新規の特徴を持ちます。
まだ開発途中ですが、運用時は、VPSレンタルサーバー費用を安く抑えられる予定で御座います。

## 現状(v0.1.0)

> ⚠️ **正直な開示**: v0.1.0時点では投稿(Post)のCRUD+OTPログイン(管理者のみ)+
> カテゴリ/コメント機能を実装している。WordPressが持つ以下の機能は**まだ一切無い**:
>
> - 固定ページ・カスタム投稿タイプ
> - テーマ・ウィジェット
> - プラグイン機構(PHPプラグイン互換レイヤは技術調査段階、未着手)
> - メディアライブラリ
> - ユーザー・ロール・権限管理(登録アカウント制・アクセス制御の細分化)
> - `aruaru-db`/PostgreSQL DUAL DB構成(現状はJSONファイル永続化のみ)

実装済みのAPI:

- `GET /healthz` — ヘルスチェック
- `POST /api/auth/request-otp` / `POST /api/auth/verify-otp` / `POST /api/auth/logout` — OTPメールログイン(管理者のみ、`RSBLOG_ADMIN_EMAIL`宛)
- `POST /api/posts` / `GET /api/posts` — 投稿の作成・一覧(ログイン必須、`GET /api/posts?category=:id`でカテゴリ絞り込み可)
- `GET /api/posts/:id` / `PUT /api/posts/:id` / `DELETE /api/posts/:id` — 投稿の取得・更新・削除(ログイン必須)
- `GET /api/categories` / `POST /api/categories` — カテゴリの一覧・新規作成(ログイン必須)
- `DELETE /api/categories/:id` — カテゴリ削除(ログイン必須)
- `POST /api/posts/:id/comments` — コメント投稿(未ログイン可、WordPressのモデレーションキューと同じく常に未承認状態で作成)
- `GET /api/posts/:id/comments?approved_only=true` — 指定投稿の承認済みコメント一覧(公開)
- `GET /api/comments` — 全コメント一覧(未承認含む、ログイン必須)
- `POST /api/comments/:id/approve` / `DELETE /api/comments/:id` — コメントの承認・削除(ログイン必須)

投稿は`draft`(下書き)/`published`(公開)の2ステータスに加え、`categories: Vec<u64>`
でカテゴリIDを複数参照可能。永続化はJSONファイル(`RSBLOG_DATA_DIR/posts.json`・
`categories.json`・`comments.json`)。詳細な設計方針・今後の予定は`CLAUDE.md`の
HANDOFFセクションを参照。

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
