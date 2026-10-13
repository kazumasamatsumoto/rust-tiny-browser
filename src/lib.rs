//! tinybrowser: 学習用の小さなブラウザ。
//!
//! 処理の流れは本物のブラウザと同じ順番にしてある。
//!
//! 1. `url`      文字列の URL を部品(スキーム・ホスト・ポート・パス・クエリ)に分ける
//! 2. `http`     TCP でつないで HTTP/1.1 の GET を送り、レスポンスを分解する
//! 3. `html`     HTML をトークンに切り(字句解析)、DOM ツリーに組み立てる(構文解析)
//! 4. `css`      CSS をトークンに切り、CSSOM(ルール・セレクタ・宣言)に組み立てる
//! 5. `renderer` 要素ごとの計算値を決め(カスケード・継承)、箱の位置と大きさを決め(レイアウト)、表示リストにする(描画)
//! 6. `js`       JavaScript をトークン → 抽象構文木 → 評価し、document.getElementById / textContent で DOM を書き換える
//! 7. `browser`  上の部品を 1 ページ分つなぐ
//!
//! 外部クレートは使わない。std だけで書いてある。
pub mod browser;
pub mod css;
pub mod html;
pub mod http;
pub mod js;
pub mod renderer;
pub mod url;
