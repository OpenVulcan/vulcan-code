Vulcan Code 0.1.11 では、フィードバック、メッセージセンター、お使いのデバイスで問題を修正する手段を追加し、OpenAI、TokenRhythm、OpenCode Go、OpenDesign のモデルを更新しました。

### このバージョンへの更新

```ui-checklist
ok | 差分更新 | 0.1.8、0.1.9、0.1.10 からの更新では変更されたファイルだけをダウンロードします。それより前のバージョンは引き続き完全更新でインストールされます
ok | アプリのデータベース | 初回起動時にアプリのデータベースをバックアップしてから新しい構造へ自動で更新します。操作は不要です
```

### フィードバックとメッセージ

```ui-checklist
ok | フィードバックを送信 | サインインすると「フィードバックを送信」から問題や提案を送れます。送信前に秘密情報、パス、ユーザー名、URL、個人情報をデバイス上で除外し、送信される内容をそのまま確認できます
ok | エラーカードから報告 | 会話が失敗したとき、エラーカードからワンクリックで報告できます。Vulcan Code が原因の可能性がある失敗だけが対象で、認証、残高、レート制限、サービス障害、タイムアウト、コンテンツポリシー、キャンセルは対象外です
ok | メッセージセンター | 「メッセージセンター」でお知らせ、個人宛てメッセージ、フィードバックへのメンテナーの返信を確認できます。重要なお知らせはバナーでも表示されます
ok | このデバイスで修正する | フォルダーを選ぶと vulcan-code のチェックアウトがプロジェクトとして準備され、修正スキル付きの会話が開きます。その会話でプルリクエストを確認するまで、何もプッシュも公開もされません
ok | GitHub への入口 | アカウントメニューに「GitHub で提案する（Issue）」と「GitHub でプルリクエストを作成」を追加しました
ok | ターミナルクライアント | ターミナルクライアントにもフィードバックページがあります
```

### モデルとプロバイダー

```ui-checklist
ok | OpenAI | 公開 API の 8 モデルが厳格な json_schema 構造化出力を宣言します。GPT-5.6 Sol/Terra/Luna、GPT-6 Sol/Luna、GPT-6.1 Terra は推論をオフにできます
ok | TokenRhythm | DeepSeek V4.1 Flash 1017、LongCat 2.0、MiniMax M3.1 Flash とテスト用モデル Egg-Alpha、Neohorse 1 9B を追加し、minimax-m2.5 と mimo-v2.5-pro を削除しました。構造化出力を宣言し、Messages モデルは思考の強度と思考オフを送るようになりました。DeepSeek V4 Pro 0813 と GLM-5.2 は Responses、Kimi K2.6 は Chat に移りました
ok | OpenCode Go | Gemini 4 Argon、Hy4 と無料の Beam、Iceman、Crouton を追加しました。10 モデルが構造化出力を宣言します
ok | OpenDesign | GPT-6 Luna は Responses、Claude Haiku 5.5 は Messages を使います。deepseek-v4.1-flash-eco、hy4-preview、minimax-m3.1-flash と 14 の Plus/Pro/Max チャットモデルを追加しました。プランに含まれる場合は GPT Image 2、Nano Banana 2/2 Lite、Seedream 5.0/5.0 Pro で画像を生成できます。思考レベルは各ベンダーが公開しているものだけを表示します
ok | メモリモデル | 思考をオフにでき構造化出力を返せるモデルが増え、より多くのモデルをメモリモデルとして選べます
```
