# 開発方針＆開発環境ルール(RWordPress)

作業ドライブは`F:\runo`。この節は[`open-raid-z`](https://github.com/aon-co-jp/open-raid-z)の
`CLAUDE.md`を正本とし、各プロジェクトへコピーして同期する方針に準じる。
GitHubリポジトリ: [aon-co-jp/RWordPress](https://github.com/aon-co-jp/RWordPress)。
VPS上の作業パス: `/root/RWordPress`(空フォルダ作成済み、2026-07-21)。

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

## HANDOFF

- **2026-07-21 プロジェクト新設(器のみ)**: GitHub空リポジトリ・
  VPS空フォルダ・ローカル作業フォルダを用意。次回、`RGit`と同じ構成
  (`Cargo.toml`+`poem`)でのブートストラップに着手する。
  - 次にすべきこと: (1) WordPressの機能のうちMVP範囲の選定(投稿+
    固定ページの表示のみ、等)、(2) プラグイン機構をどう再設計するか
    の方針、(3) データモデル設計(DB選定含む)。
