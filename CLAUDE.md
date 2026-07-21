# 開発方針＆開発環境ルール(RS-Blog)

作業ドライブは`F:\runo`。この節は[`open-raid-z`](https://github.com/aon-co-jp/open-raid-z)の
`CLAUDE.md`を正本とし、各プロジェクトへコピーして同期する方針に準じる。
GitHubリポジトリ: [aon-co-jp/RS-Blog](https://github.com/aon-co-jp/RS-Blog)。
VPS上の作業パス: `/root/RS-Blog`(空フォルダ作成済み、2026-07-21)。

## このプロジェクトの役割

[WordPress](https://wordpress.org/)(PHP製)の、ハイスピード・
ハイセキュリティ・省メモリなRust+[poem](https://github.com/poem-web/poem)
(RPoem)版を目指す。`RGit`(Gitea相当)・`RJSON`(JSON処理)と同じ
`aon-co-jp`エコシステムの一員。

> ⚠️ **正直な開示**: 2026-07-21時点でコード未着手(このCLAUDE.mdのみの
> 状態)。実装が追いつくまでは「WordPressの代替品」を名乗らず、
> 進捗をこのHANDOFFに正直に記録する。

## 着手時に踏襲すべき既存プロジェクトの設計方針

- **`RGit`**(git smart HTTP・OTPログイン・アクセス制御・容量ベースの
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
  **1つずつ順番に、`RGit`と同じ深さまで作り込んでから次へ**進める。
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

- **2026-07-21 プロジェクト新設(器のみ)**: GitHub空リポジトリ・
  VPS空フォルダ・ローカル作業フォルダを用意。次回、`RGit`と同じ構成
  (`Cargo.toml`+`poem`)でのブートストラップに着手する。
  - 次にすべきこと: (1) 3プロジェクトのうちどれから着手するか決定、
    (2) WordPressの機能のうちMVP範囲の選定(投稿+固定ページの表示
    のみ、等)、(3) PHPプラグイン互換レイヤの技術調査(embed-PHP系
    クレートの調査等)、(4) `aruaru-db`との接続方式の設計。
