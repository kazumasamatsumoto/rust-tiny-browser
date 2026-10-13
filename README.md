# tinybrowser — Rust で作る小さなブラウザ(学習用)

Dev Study のハンズオン「Rust で小さなブラウザを作る」(https://study.ai-search-match.org/programming/rust-browser)の完成版です。
外部クレートは使っていません(std だけ)。
リポジトリ: https://github.com/kazumasamatsumoto/rust-tiny-browser (レッスンの本文・用語集・実行欄はサイト側にあります)

```
cargo test                                   # 全部のテスト(78 本)
cargo run -- --file sample/index.html        # ローカルの HTML を描く
cargo run -- --file sample/index.html --dom --layout --display-list   # 途中の木も見る
(cd sample && python3 -m http.server 8000)   # 別の端末で簡易サーバーを立てて…
cargo run -- http://localhost:8000/index.html --headers              # HTTP で取って描く
cargo run -- http://example.com/                                     # 本物のサーバーからも
```

## 構成

```
src/
  url.rs            URL を部品に分ける(WHATWG URL Standard の http だけ)
  http.rs           TCP で GET を 1 回送り、レスポンスを分解する(RFC 9112)
  html/token.rs     HTML のトークン
  html/tokenizer.rs HTML の字句解析(HTML Standard 13.2.5 の状態機械の一部)
  html/dom.rs       DOM ツリー(Rc / Weak / RefCell)
  html/parser.rs    HTML の構文解析(挿入モードと開いている要素のスタック)
  css/token.rs      CSS のトークン(CSS Syntax Level 3)
  css/tokenizer.rs  CSS の字句解析
  css/cssom.rs      CSSOM(ルール・セレクタ・宣言)と詳細度
  css/parser.rs     CSS の構文解析
  renderer/style.rs 計算値(カスケード・継承・UA 既定)
  renderer/layout.rs 箱の木と位置・大きさ(通常フローだけ)
  renderer/paint.rs 表示リストと文字のキャンバス
  js/lexer.rs       JavaScript の字句解析
  js/ast.rs         抽象構文木
  js/parser.rs      再帰下降の構文解析
  js/runtime.rs     評価(環境・関数・ブラウザ API)
  browser.rs        1 ページ分をつなぐ(DOM → CSSOM → script → layout → paint)
  main.rs           コマンドライン
steps/              各ステップを 1 ファイルにまとめたもの(rustc 1 本で動く。tools/assemble.py で生成)
steps/run/          同じものからテストを抜いた版(サイトの実行ボタンが使う)
sample/index.html   試し用のページ
tools/              生成スクリプト
```

## 1 ファイル版の使い方

```
rustc --edition 2021 steps/step04_html_tokenizer.rs && ./step04_html_tokenizer
rustc --edition 2021 --test steps/step04_html_tokenizer.rs -o t && ./t     # そのステップまでのテスト
```

## 作っていないもの(本物との差)

HTTPS、リダイレクト、文字参照(&amp;)、コメントと DOCTYPE の解釈、RCDATA/RAWTEXT の各状態、結合子つきのセレクタ、
margin/padding/border/float/flex、フォントの実測、画像、イベントとイベントループ、if/for/オブジェクト/配列/this/例外、
script の実行タイミング(本物は出会った時点、これは DOM 完成後)、マルチプロセス、キャッシュ、Cookie。
