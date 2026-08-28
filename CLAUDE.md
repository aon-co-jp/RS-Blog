# 開発方針＆開発環境ルール(RS-Blog)

作業ドライブは`F:\runo`。この節は[`open-raid-z`](https://github.com/aon-co-jp/open-raid-z)の
`CLAUDE.md`を正本とし、各プロジェクトへコピーして同期する方針に準じる。
GitHubリポジトリ: [aon-co-jp/RS-Blog](https://github.com/aon-co-jp/RS-Blog)。
VPS上の作業パス: `/root/repository/RS-Blog`(2026-08-28、VPSディレクトリ
構成整理によりリポジトリは`/root/repository/`配下へ移動。旧パス
`/root/RS-Blog`はシンボリックリンクとして残しており、systemdサービス等
既存の参照はそのまま機能する)。

## このプロジェクトの役割

[WordPress](https://wordpress.org/)(PHP製)の、ハイスピード・
ハイセキュリティ・省メモリなRust+[poem](https://github.com/poem-web/poem)
(RPoem)版を目指す。`RS-Git`(Gitea相当)・`RJSON`(JSON処理)と同じ
`aon-co-jp`エコシステムの一員。

> ⚠️ **正直な開示**: 2026-07-21時点でコード未着手(このCLAUDE.mdのみの
> 状態)。実装が追いつくまでは「WordPressの代替品」を名乗らず、
> 進捗をこのHANDOFFに正直に記録する。

## 着手時に踏襲すべき既存プロジェクトの設計方針

- **`RS-Git`**(git smart HTTP・OTPログイン・アクセス制御・容量ベースの
  自動判定)を先行実装として参照。「正直な開示」「段階的実装」
  「型チェックだけで完了と報告しない・実機検証必須」の3方針は共通。
- **`open-easy-web`**(PHP実行対応の自己学習AI判定、`php_detector.rs`)
  は関連する既存実装として参照価値あり——WordPressはPHP製のため、
  PHP実行環境との連携・非依存化の両面を検討する際の先行事例になる。

## WordPressの主要機能(着手時の優先順位付けの参考)

- 投稿・固定ページ・カスタム投稿タイプ
- テーマ・ウィジェット
- プラグイン機構(拡張性)
- ユーザー・ロール・権限管理
- メディアライブラリ
- REST API

## 方針決定事項(2026-07-21、ユーザー確認済み)

- **着手順番**: `RS-Chiketto`・`RS-Blog`・`RS-EC`は同時並行ではなく
  **1つずつ順番に、`RS-Git`と同じ深さまで作り込んでから次へ**進める。
  どれを最初にするかは次回セッション冒頭で決定。
- **データベース**: `aruaru-db`(ZFS互換・ACID互換のRust製DB)を採用、
  3プロジェクトで統一する。加えて**PostgreSQLとのDUAL DATABASE構成も
  可能にする**(ユーザー指示、2026-07-21追記)——`open-runo`/RPoemの
  「4層4重」DUAL DB思想と同じ方針、設定で切り替え可能にする。
- **「分身の術」構成でDB層を共有する**(ユーザー指示、2026-07-21追記):
  `open-web-server`・`aruaru-llm`・RPoem/RCosmoと同じ設計思想により、
  `aruaru-db`/PostgreSQL接続は**1インスタンスを複数ドメインが共有**し、
  ドメイン追加のたびに個別インストールは不要とする。実装は`aruaru-llm`
  の`src/tenants.rs`(`TenantRegistry`)と同じパターン。**管理は
  `open-easy-web`側から行う**(`appserver_registration.rs`に
  `RS-Blog`用の`AppServerKind`variantを追加する形)。
  **非同期・マルチCPU/マルチコア/マルチスレッド対応**: `#[tokio::main]`
  は既定の`multi_thread`フレーバー、CPU負荷の高い処理は`rayon`で
  全論理コアへ並列ディスパッチする。
- **PHPプラグイン互換性**: **既存のPHPプラグイン資産を実行できる
  互換レイヤも目指す**(機能相当の新規実装だけでは終わらせない、
  ユーザー指示)。難易度が非常に高いことは認識した上で、`open-easy-web`
  の`php_detector.rs`(PHP実行環境検知)を参考に、まずPHP実行環境との
  連携方式(埋め込みPHPインタプリタ呼び出し等)の技術調査から始める。

## HANDOFF

- **2026-08-28 ログイン4択方式の横展開: QR確認ログイン+`login_mode`管理API+
  `qr-confirm.html`を追加(open-english/open-easy-webと同じパターン)**:
  `F:\runo\PORTING.md`の「ログイン方式4択…横展開チェックポイント」節の
  対応、残り6リポジトリのうち最初の1つとして着手。
  1. **「パスワード無し」モードは実装しなかった(正直な設計判断)**:
     RS-Blogは公開ブログの管理者アカウントであり、侵害されると投稿の
     改ざん・削除が可能——open-easy-web(VPS/サイト管理)と同じ理由で、
     コンテンツ完全性に関わる重要操作を扱うため認証の完全省略は
     不適切と判断した。実装したのは`otp`(既定、既存のメールOTP単体)・
     `qr`(事前のOTP検証なしでQR撮影のみ)・`otp_qr`(OTP検証成功後、
     QR確認を第二要素として要求する真の2FA)の3方式のみ。
  2. **`src/auth.rs`**: `login_mode`(`Mutex<String>`、既定`"otp"`)・
     `qr_logins`(QRログインセッション、3分TTL・1回限り)を`AuthStore`へ
     追加。`start_qr_login`/`qr_login_masked_email`/`confirm_qr_login`/
     `qr_login_status`/`finish_qr_login`(open-english/open-easy-webと
     同じ設計)。RS-Blogは登録アカウント制を持たず単一の管理者
     (`RSBLOG_ADMIN_EMAIL`)のみのため、open-easy-webの「TOTP登録済み
     アカウントのみqr可」という前提条件は不要——QR単体ログインは
     事前準備なしに常時利用可能。
  3. **`src/main.rs`**: `GET/POST /api/auth/login-mode`(GETは未ログイン
     可、POSTは管理者セッション必須——最初の1回は既定の`otp`でログイン
     してから切り替える)・`POST /api/auth/qr-login/{start,confirm,
     finish}`・`GET /api/auth/qr-login/{whoami,status}`・
     `GET /qr-confirm.html`(スマホ/タブレット/WEBカメラで開くと
     ボタン操作なしに自動承認される単体完結ページ、open-english版を
     RS-Blogブランディングへ差し替えたもの)。既存の`verify_otp`
     ハンドラを拡張し、`login_mode`が`otp_qr`の場合はセッションを
     即発行せず`{"qr_required": true, "qr_id": ...}`を返すようにした。
  4. **実機検証(型チェック・ビルド成功だけで完了と報告しない方針の
     徹底)**: `cargo build`成功(警告0件)。`cargo test`
     **26件全green**(新規11件: `auth::tests`5件+`tests::`6件、
     既存15件から回帰なし)。新規6件のうち3件は実HTTP経由の統合テスト
     (`qr_only_login_full_flow_over_real_http`——start→whoami→status→
     confirm→status→finish→セッション発行→保護API到達→二重finish拒否
     まで一気通貫、`otp_qr_mode_requires_qr_confirmation_as_second_
     factor_over_real_http`——OTP検証→QR確認→finish→保護API到達、
     `qr_login_start_is_rejected_when_login_mode_is_otp`)。さらに
     実バイナリを起動し、curl経由で`GET /`(200)・
     `GET /api/auth/login-mode`(`{"login_mode":"otp"}`)・
     `POST /api/auth/qr-login/start`(otpモード時403)・
     `GET /qr-confirm.html`(RS-Blogブランディング付きHTML)・
     未ログインでの`POST /api/auth/login-mode`(401)・SMTP未設定の
     `POST /api/auth/request-otp`(503、既存の正直な劣化動作)を
     実HTTPで確認した。
  5. **正直な開示・未実施**: この開発機にSMTPサーバーが用意できない
     ため、実際のメール送信を経由した`otp_qr`モードのフルE2E
     (実メール受信→OTP入力→QR確認→ログイン)はcurlでは検証していない
     (`cargo test`内の`TestClient`による実HTTP相当の統合テストで代替、
     既存のOTP関連検証と同じ制約)。実際のQRコード画像生成・スマホ実機
     でのカメラ撮影によるURL起動もこのパスでは未検証(QRは単に短命URLを
     埋め込むだけの設計のため、URL自体の起動確認は`qr-confirm.html`の
     動作確認で代替——実際にQR画像へエンコードする処理はクライアント側
     〈将来のブラウザUI〉の実装範囲であり、v0.1.0時点のRS-Blogには
     まだブラウザUI自体が無い〈JSON APIのみ〉ため対象外)。
  - 次にすべきこと: (1) 残り5リポジトリ(RS-EC/RS-Ops/open-gitea/
    open-redmine/rs-sync)への同様の横展開、(2) 実SMTP環境での
    `otp_qr`フルE2E検証、(3) RS-Blog自体のブラウザUI実装時に、QRコード
    画像のレンダリング(`qrcode`crate等)を追加する。

- **2026-07-22 実バイナリ起動による実HTTPスモークテストを実施(既存の
  「未実施」だった項目の解消)**: `cargo build`(警告0件)・`cargo test`
  (**16件全green**、前回記録の11件から投稿以外にカテゴリ/タグ/固定ページ/
  コメントモデレーションのテストが増えていた)を確認した上で、
  `target/debug/rs-blog.exe`を`RSBLOG_PORT=8199`/一時`RSBLOG_DATA_DIR`で
  実際に起動し、curlで以下を実HTTP確認した: (1) `GET /`が200・本文に
  `RS-Blog`の文字列を含む、(2) `GET /healthz`が`ok`、(3) 未ログインでの
  `GET /api/posts`が401、(4) `POST /api/auth/request-otp`が
  (`RSBLOG_SMTP_*`未設定のサンドボックスのため)503——ハンドラの
  `state.smtp.is_none()`分岐が実際にこの通り応答することを実機で確認。
  **正直な開示・残る検証の限界**: このサンドボックスに実SMTPサーバー
  (`RSBLOG_SMTP_*`)が用意できないため、OTPメール実送信→`verify-otp`→
  トークン取得→`POST/GET/PUT/DELETE /api/posts`の認証つき一気通貫は
  今回も実機curlでは未実施のまま(`cargo test`内の`TestClient`による
  インメモリE2E、5件+新規カテゴリ/タグ/固定ページ/コメント関連が
  この部分を代替検証している)。次回、実SMTP環境(VPS等)が使える
  セッションで、OTP発行→受信→`verify-otp`→Bearerトークンでの投稿CRUDを
  実際のメール受信込みでcurl検証すること。
  検証後、作業ツリーはクリーン(コミット対象の変更なし)だったため、
  このHANDOFF更新のみをコミットしてpushする。

- **2026-07-21 プロジェクト新設(器のみ)**: GitHub空リポジトリ・
  VPS空フォルダ・ローカル作業フォルダを用意。次回、`RS-Git`と同じ構成
  (`Cargo.toml`+`poem`)でのブートストラップに着手する。
  - 次にすべきこと: (1) 3プロジェクトのうちどれから着手するか決定、
    (2) WordPressの機能のうちMVP範囲の選定(投稿+固定ページの表示
    のみ、等)、(3) PHPプラグイン互換レイヤの技術調査(embed-PHP系
    クレートの調査等)、(4) `aruaru-db`との接続方式の設計。

- **2026-07-21(続き) v0.1.0ブートストラップ完了: 投稿CRUD+OTP認証**
  (`RS-Chiketto`のv0.1.0ブートストラップと全く同じパターンを踏襲):
  1. `RS-Chiketto`の`src/auth.rs`/`src/mail.rs`をそのまま移植(OTP
     ログイン機構、環境変数名のみ`RSBLOG_*`に変更)。v0.1.0時点では
     管理者アカウント(`RSBLOG_ADMIN_EMAIL`)のみログイン可能(`RS-Git`/
     `RS-Chiketto`にある登録アカウント制・アクセス制御の細分化は
     まだ移植していない、次回以降の増分)。
  2. 投稿(Post)のCRUD: `POST/GET /api/posts`・
     `GET/PUT/DELETE /api/posts/:id`。ステータスは`draft`/`published`の
     2値。永続化はJSONファイル(`RSBLOG_DATA_DIR/posts.json`、`aruaru-db`/
     PostgreSQL DUAL DB構成への移行はまだ未着手——今回は動くMVPを優先)。
     `RS-Chiketto`のチケットCRUDには無い`DELETE`ハンドラも追加した。
  3. **検証**: `cargo build`は警告0件で成功(2m28s)。`cargo test`は
     **11件全green**(`auth::tests`が6件、投稿CRUDのハンドラテストが
     `poem::test::TestClient`を使って5件——未ログインでの`GET /api/posts`
     が`401`になること、投稿の作成→一覧→取得→更新(ステータス変更)→
     削除→削除後の`404`確認、空タイトルでの作成が`400`になること、を
     それぞれ検証)。テストは`poem`の`test`featureを`dev-dependencies`に
     追加して実現(`RS-Chiketto`側のCargo.tomlには無かった追加、
     `poem::test`モジュールがデフォルトでは無効なため)。
     **注意**: 実バイナリを起動してのcurlベースの実HTTPスモークテスト
     (`RS-Chiketto`のHANDOFFにある「未ログインで401→OTPログイン→
     作成201→一覧→更新」の一連)は今回**未実施**——`cargo test`内の
     `TestClient`によるインメモリE2Eテストで代替した。実SMTP環境が
     手元に無かったため、次回実機(VPS等)でのデプロイ時に必ず
     curlでの実HTTPスモークテストを行うこと。
  4. `install.sh`/`install.ps1`/`.github/workflows/release.yml`は
     `RS-Chiketto`版をそのままリネーム移植(`chiketto`→`blog`、
     `RSCHIKETTO`→`RSBLOG`、ポートは`8101`)。muslターゲット
     (`x86_64-unknown-linux-musl`)による静的リンクLinuxバイナリ配布も
     同じ方針。
  5. `README.md`に「現状」セクションを追加(既存のマーケティング文・
     開発開始日は変更せず)。
  - **次にすべきこと**: (1) 実機(VPS)でのcurlベース実HTTPスモーク
    テスト(OTPメール実送信含む)、(2) `RS-Git`にある登録アカウント制・
    アクセス制御(閲覧/編集の個別許可)の移植、(3) 固定ページ・
    カスタム投稿タイプ・テーマ・ウィジェット等の追加機能、
    (4) PHPプラグイン互換レイヤの技術調査、(5) `aruaru-db`/PostgreSQL
    DUAL DB構成への移行(現状はJSONファイル永続化)、(6) GitHubへの
    初回push・VPSデプロイ(`runo.tokyo/blog`、ポート`8101`)。


## 同時並行開発の対象プロジェクト(2026-07-21、ユーザー指示・拡張版)

`RS-Chiketto`・`RS-Blog`・`RS-EC`(この3プロジェクト自身、着手順は
「1つずつ順番に」の方針のまま)に加えて、以下の既存プロジェクトを
**同時に開発を進め、完成度を高めていく**:

- [open-raid-z](https://github.com/aon-co-jp/open-raid-z) — 開発ルールの
  正本。3プロジェクトの`CLAUDE.md`もここの記述と同期を取る。
- [aruaru-db](https://github.com/aon-co-jp/aruaru-db) — ZFS互換・ACID
  互換のRust製DB。3プロジェクトが採用する「分身の術」DB共有構成の実体。
- [open-cuda](https://github.com/aon-co-jp/open-cuda) — GPU抽象化・
  GEMM/Attention計算基盤(`opencuda-blas`/`opencuda-bert`)。
- [aruaru-llm](https://github.com/aon-co-jp/aruaru-llm) — 上記
  `open-cuda`を使った実装例(bag-of-words→文埋め込みベースの意図分類へ
  移行済み)。3プロジェクトが将来AI機能を持つ際の先行実装として参照。
- [open-web-server](https://github.com/aon-co-jp/open-web-server) —
  「分身の術」構成(1インスタンスを複数ドメインが共有)の基盤実装、
  Nginx/Apacheハイブリッド仕様のWebサーバー。
- [open-cosmo](https://github.com/aon-co-jp/open-cosmo) — 関連する
  Webサーバー/フロントエンド基盤(詳細は同リポジトリのCLAUDE.md参照)。
- [RPoem](https://github.com/aon-co-jp/RPoem) — アプリケーションサーバー
  層(旧poem-cosmo-tauri)。`open-raid-z`とVersionlessAPIによる
  バージョンレス運用、`aruaru-db`とのDUAL DATABASE構成の先行実装。

- Python製AIライブラリのRust移植ハイブリッド/トライブリッド版
  (マーケティング調査での1〜6位、vLLM/Transformers/NumPy/PyTorch互換/
  scikit-learn/Whisper相当の良いとこ取り)——**Rustを基本とし、必要なら
  `RPoem`(アプリケーションサーバー層)も併用する**(ユーザー指示、
  2026-07-21追記)。`open-cuda`ワークスペース内の`opencuda-blas`
  (NumPy相当)・`opencuda-bert`(Transformers推論パス相当、実装済み)が
  このトライブリッド化の実体。今後の`opencuda-llm`(vLLM相当、生成
  デコーダ追加時)を、必要であれば`RPoem`上のHTTPサービスとして
  提供することも視野に入れる。

**理由**: これらは3プロジェクトが実際に依存する基盤コンポーネント
(DB層・GPU計算基盤・「分身の術」共有構成・アプリケーションサーバー層)
であり、基盤側の完成を待ってから3プロジェクトに着手するのではなく、
実際に統合しながら並行して育て、エコシステム全体の完成度を高めていく
方針とする。

## 公開先・配布方針(2026-07-21、ユーザー確認済み、着手時に反映すること)

- **公開パス**: `runo.tokyo/blog`(`RS-Git`の`runo.tokyo/rgit`・
  `RS-Chiketto`の`runo.tokyo/chiketto`と同じパス方式、VPS上の
  ポートは`8101`)。
- **クロスプラットフォーム配布**: AlmaLinux・Ubuntu・Debian・Fedora・
  RHEL等の主要Linuxディストリ、Windows・Windows Server向けに、
  インストーラー付きのビルド済みバイナリをGitHub Releasesで配布する
  (ユーザー指示、`RS-Chiketto`の
  `.github/workflows/release.yml`・`install.sh`・`install.ps1`を
  雛形として踏襲すること)。

## HANDOFF追記(2026-07-31) インストーラーの電源プロファイル選択機能(未実装、エコシステム標準方針として記録)

`open-raid-z`のCLAUDE.md(全リポジトリ共通の設計思想セクション)に、
インストーラー(`install.sh`/`install.ps1`等)実行時に以下3つの電源
プロファイルを選択させる標準方針を追記した(ユーザー指示、2026-07-31):

1. **省電力(Power-saving)**: CPU使用率・ポーリング間隔を抑えた低負荷設定。
2. **省メモリ(Low-memory)**: メモリ確保量・キャッシュサイズを抑えた設定。
3. **常時電源接続(Always-on)**: 上記の抑制を行わないフル性能設定。
   **この場合のみ**ハードウェアアクセラレータ(NPU/GPU)のサポートを
   自動検出・自動有効化する(`open-cuda`の`GpuDevice`抽象化を利用)。

**正直な開示**: このリポジトリのインストーラーへの実装はまだ未着手。
実装時は`open-raid-z/CLAUDE.md`の該当節、および先行実装予定の
`open-redmine/CLAUDE.md`を参照し、`open-cuda`側のGPU/NPUベンダー検出
ロジックを再利用すること(車輪の再発明を避ける)。
- 次にすべきこと: このリポジトリの`install.sh`/`install.ps1`に上記3
  プロファイルの選択機能を追加する。
