#!/usr/bin/env python3
"""クレートのモジュールを「1 ファイルで動く Rust プログラム」に組み立てる。

各ステップ = 使うモジュールの一覧 + そのステップを見せる main()。
出力は steps/<id>.rs。rustc 1 本でコンパイルでき、`rustc --test` でそのモジュールのテストも走る。
教材(サイト)に載せるコードとクレートの中身が食い違わないように、手で写さずここから作る。
"""
import os
import re
import sys

ROOT = os.path.join(os.path.dirname(__file__), "..")
SRC = os.path.join(ROOT, "src")
OUT = os.path.join(ROOT, "steps")

HEADER = """// {title}
//
// このファイル 1 つで動きます:
//   実行:      rustc --edition 2021 {id}.rs && ./{id}
//   テスト:    rustc --edition 2021 --test {id}.rs -o {id}_test && ./{id}_test
// cargo のプロジェクトでは、各 `mod` の中身がそのまま src/ 以下のファイルになります。
#![allow(dead_code)] // 部品を全部 1 ファイルに入れているので、このステップで使わない関数もある

"""


def read_module(path, strip_tests=False):
    with open(os.path.join(SRC, path), encoding="utf-8") as f:
        text = f.read().rstrip("\n")
    if strip_tests:
        # テストはどのファイルでも末尾の `#[cfg(test)] mod tests { ... }` だけ。そこから先を落とす
        i = text.find("\n#[cfg(test)]")
        if i >= 0:
            text = text[:i].rstrip("\n")
    return text


def indent(text, n=4):
    pad = " " * n
    return "\n".join((pad + line) if line.strip() else "" for line in text.split("\n"))


def mod_block(name, body, depth=0, public=False):
    vis = "pub " if public else ""
    return indent(f"{vis}mod {name} {{\n{indent(body)}\n}}", depth * 4)


# モジュールの木。値が str ならファイル、dict なら子モジュール
TREE = {
    "url": "url.rs",
    "http": "http.rs",
    "html": {"token": "html/token.rs", "tokenizer": "html/tokenizer.rs", "dom": "html/dom.rs", "parser": "html/parser.rs"},
    "css": {"token": "css/token.rs", "tokenizer": "css/tokenizer.rs", "cssom": "css/cssom.rs", "parser": "css/parser.rs"},
    "renderer": {"style": "renderer/style.rs", "layout": "renderer/layout.rs", "paint": "renderer/paint.rs"},
    "js": {"lexer": "js/lexer.rs", "ast": "js/ast.rs", "parser": "js/parser.rs", "runtime": "js/runtime.rs"},
    "browser": "browser.rs",
}


def build_modules(selected, strip_tests=False):
    """selected: ["url", "html/token", ...] の順に、ネストした mod ブロックを作る"""
    out = []
    # 親モジュールごとにまとめる(順序は selected の初出順)
    groups = {}
    for sel in selected:
        groups.setdefault(sel.split("/")[0], []).append(sel)
    for top, items in groups.items():
        if isinstance(TREE[top], str):
            out.append(mod_block(top, read_module(TREE[top], strip_tests)))
        else:
            inner = []
            for sel in items:
                sub = sel.split("/")[1]
                inner.append(f"pub mod {sub} {{\n{indent(read_module(TREE[top][sub], strip_tests))}\n}}")
            out.append(f"mod {top} {{\n{indent(chr(10).join(inner))}\n}}")
    return "\n\n".join(out)


HTML_ALL = ["html/token", "html/tokenizer", "html/dom", "html/parser"]
CSS_ALL = ["css/token", "css/tokenizer", "css/cssom", "css/parser"]

STEPS = [
    {
        "id": "step01_url",
        "title": "ステップ 1: URL を部品に分ける",
        "modules": ["url"],
        "main": r'''
fn main() {
    let inputs = [
        "http://example.com",
        "http://localhost:8000/docs/index.html?q=rust&page=2#top",
        "HTTP://Example.COM:8080",
        "https://example.com",
        "example.com",
        "http://example.com:80a/",
    ];
    for input in inputs {
        println!("{input}");
        match url::parse(input) {
            Ok(u) => {
                println!("  scheme = {}", u.scheme);
                println!("  host   = {}", u.host);
                println!("  port   = {}", u.port);
                println!("  path   = {}", u.path);
                println!("  query  = {:?}", u.query);
                println!("  → request-target: {}   Host ヘッダー: {}", u.request_target(), u.host_header());
            }
            Err(e) => println!("  → エラー: {e}"),
        }
        println!();
    }
}
''',
    },
    {
        "id": "step02_http_message",
        "title": "ステップ 2: HTTP のリクエストを組み立て、レスポンスを分解する(通信はしない)",
        "modules": ["url", "http"],
        "main": r'''
fn main() {
    // 1. 送る側: リクエストの文字列。改行は CRLF(\r\n)。見えるように置き換えて表示する
    for target in ["http://example.com/index.html?a=1", "http://localhost:8000/"] {
        let u = url::parse(target).unwrap();
        println!("── {target} に送るリクエスト ──");
        print!("{}", http::build_request(&u).replace("\r\n", "\\r\\n\n"));
        println!();
    }

    // 2. 受ける側: サーバーが返してきた文字列を分解する
    let raw = "HTTP/1.1 200 OK\r\n\
               Content-Type: text/html; charset=utf-8\r\n\
               Content-Length: 27\r\n\
               Connection: close\r\n\
               \r\n\
               <h1>hello</h1>\n<p>world</p>\n";
    println!("── レスポンスの分解 ──");
    match http::HttpResponse::parse(raw) {
        Ok(r) => {
            println!("version      = {}", r.version);
            println!("status_code  = {}", r.status_code);
            println!("reason       = {}", r.reason_phrase);
            for h in &r.headers {
                println!("header       = {}: {}", h.name, h.value);
            }
            println!("Content-Type = {:?}  (名前の大文字小文字は区別しない)", r.header("content-type"));
            println!("body         = {:?}", r.body);
        }
        Err(e) => println!("エラー: {e}"),
    }

    // 3. chunked(長さが先に分からないとき)
    let chunked = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n7\r\n, world\r\n0\r\n\r\n";
    println!("\n── chunked の本文 ──");
    println!("{:?}", http::HttpResponse::parse(chunked).map(|r| r.body));

    // 4. 壊れたレスポンス
    println!("\n── 壊れたレスポンス ──");
    for bad in ["", "<html>not http</html>", "HTTP/1.1 200 OK\r\nno-colon-here\r\n\r\n"] {
        println!("{:?} → {}", bad, match http::HttpResponse::parse(bad) { Ok(_) => "OK".to_string(), Err(e) => e.to_string() });
    }
}
''',
    },
    {
        "id": "step03_http_fetch",
        "title": "ステップ 3: TCP でつないで GET を送り、本物のサーバーから受け取る(ネットワークが要る)",
        "modules": ["url", "http"],
        "main": r'''
fn main() {
    // 使い方: ./step03_http_fetch http://localhost:8000/index.html
    // ローカルサーバーは  python3 -m http.server 8000  で立つ(index.html を置いたフォルダで)
    let target = std::env::args().nth(1).unwrap_or_else(|| "http://example.com/".to_string());
    let u = match url::parse(&target) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("URL が不正: {e}");
            std::process::exit(1);
        }
    };
    println!("→ {} に接続して GET {} を送ります", u.host_header(), u.request_target());
    match http::fetch(&u) {
        Ok(r) => {
            println!("{} {} {}", r.version, r.status_code, r.reason_phrase);
            for h in &r.headers {
                println!("  {}: {}", h.name, h.value);
            }
            println!("--- body({} 文字。先頭 400 文字) ---", r.body.chars().count());
            println!("{}", r.body.chars().take(400).collect::<String>());
        }
        Err(e) => eprintln!("失敗: {e}"),
    }
}
''',
    },
    {
        "id": "step04_html_tokenizer",
        "title": "ステップ 4: HTML を状態機械でトークンに切る",
        "modules": ["html/token", "html/tokenizer"],
        "main": r'''
use html::token::HtmlToken;
use html::tokenizer::HtmlTokenizer;

fn show(html: &str) {
    println!("── 入力: {html:?}");
    // 連続する文字トークンは見やすいようにまとめて表示する(トークン自体は 1 文字ずつ)
    let mut text = String::new();
    for token in HtmlTokenizer::new(html) {
        if let HtmlToken::Char(c) = token {
            text.push(c);
            continue;
        }
        if !text.is_empty() {
            println!("   Char × {}: {:?}", text.chars().count(), text);
            text.clear();
        }
        println!("   {token:?}");
    }
    println!();
}

fn main() {
    show("<p>hi</p>");
    show(r#"<a href="/x" title='t' target=_blank>link</a>"#);
    show("<input disabled required/>");
    show("<DIV CLASS=\"A\">大文字は小文字に</DIV>");
    show("a < b は タグではない");
    show("<script>if (a < b) { s = '</div>'; }</script>");
    show("<!DOCTYPE html><!-- コメント --><br>");
}
''',
    },
    {
        "id": "step05a_dom_nodes",
        "title": "ステップ 5a: DOM のノードを Rust で持つ(Rc・Weak・RefCell)",
        "modules": ["html/token", "html/dom"],
        "main": r'''
use std::rc::Rc;
use html::dom::Node;
use html::token::Attribute;

fn main() {
    // 手でツリーを組む: <body><h1 id="t">Hi</h1><p>a <b>b</b></p></body>
    let body = Node::element("body", vec![]);
    let h1 = Node::element("h1", vec![Attribute::new("id", "t")]);
    Node::append_child(&h1, Node::text("Hi"));
    let p = Node::element("p", vec![]);
    Node::append_child(&p, Node::text("a "));
    let b = Node::element("b", vec![]);
    Node::append_child(&b, Node::text("b"));
    Node::append_child(&p, b);
    Node::append_child(&body, h1.clone());
    Node::append_child(&body, p.clone());

    println!("── ツリー ──");
    print!("{}", Node::dump(&body));

    println!("\n── 親子の参照 ──");
    println!("p の親 = <{}>", p.parent().unwrap().tag().unwrap());
    println!("body の親 = {:?}(根なので無い)", body.parent().map(|n| n.tag().map(String::from)));
    println!("h1 の所有者の数 = {}(変数 h1 と、body の children)", Rc::strong_count(&h1));
    println!("body の所有者の数 = {}(子は親を Weak で指すので増えない)", Rc::strong_count(&body));

    println!("\n── textContent ──");
    println!("p.text_content() = {:?}", p.text_content());
    Node::set_text_content(&p, "置き換えた");
    println!("set_text_content 後: {:?}  子の数 = {}", p.text_content(), p.children().len());

    println!("\n── getElementById ──");
    println!("{:?}", Node::get_element_by_id(&body, "t").map(|n| n.text_content()));
    println!("{:?}", Node::get_element_by_id(&body, "nothing").map(|n| n.text_content()));

    println!("\n── 解放 ──");
    let weak_h1 = Rc::downgrade(&h1);
    drop(h1);
    println!("h1 変数を捨てても body が持っている: {}", weak_h1.upgrade().is_some());
    drop(body);
    println!("body を捨てると h1 も消える: {}", weak_h1.upgrade().is_none());
}
''',
    },
    {
        "id": "step05_dom",
        "title": "ステップ 5: トークンから DOM ツリーを組み立てる(挿入モードと開いている要素のスタック)",
        "modules": HTML_ALL,
        "main": r'''
use html::dom::Node;
use html::parser::HtmlParser;

fn show(title: &str, html: &str) {
    println!("══ {title} ══");
    println!("入力: {html:?}");
    let (doc, trace) = HtmlParser::new(html).run();
    println!("--- 挿入モードの遷移 ---");
    for t in &trace {
        println!("  {t}");
    }
    println!("--- DOM ---");
    print!("{}", Node::dump(&doc));
    println!();
}

fn main() {
    show("ふつうの文書", "<html><head><title>T</title></head><body><h1 id=\"t\">Hi</h1><p>a <a href=\"/x\">link</a></p></body></html>");
    show("タグを省略しても html / head / body ができる", "hello");
    show("<p> の閉じ忘れ: 次の <p> が来たら前の <p> は閉じる", "<p>one<p>two");
    show("対応しない終了タグは無視", "<p>a</b>b</p>");
    show("<div> を閉じると中の <p> も閉じる", "<div><p>x</div><span>y</span>");
    let doc = HtmlParser::parse("<p id=\"a\">x</p><p id=\"b\">y</p>");
    println!("getElementById(\"b\") → {:?}", Node::get_element_by_id(&doc, "b").map(|n| n.text_content()));
}
''',
    },
    {
        "id": "step06_css_tokenizer",
        "title": "ステップ 6: CSS をトークンに切る",
        "modules": ["css/token", "css/tokenizer"],
        "main": r'''
use css::tokenizer::CssTokenizer;

fn show(css: &str) {
    println!("── 入力: {css:?}");
    for t in CssTokenizer::new(css) {
        println!("   {t:?}");
    }
    println!();
}

fn main() {
    show("p { color: red; }");
    show("#main .note { font-size: 16px; width: 50%; margin: -2px }");
    show("@media (max-width: 600px) { a { color: #ff0000 } }");
    show("font-family: \"Noto Sans\", sans-serif; /* コメントは消える */");
    show("background: rgb(1, 2, 3)");
}
''',
    },
    {
        "id": "step07_cssom",
        "title": "ステップ 7: トークンから CSSOM(ルール・セレクタ・宣言)を組み立てる",
        "modules": ["html/token", "html/dom"] + CSS_ALL,
        "main": r'''
use css::parser::CssParser;

fn main() {
    let css = r#"
        /* 見出しは紺、リンクは下線なし */
        h1, h2 { color: navy; font-size: 24px }
        a { text-decoration: none }
        #main { background-color: lightyellow }
        .note { color: gray }
        div p { color: green }            /* 結合子は未対応 → どこにも当たらない */
        p { color red; font-size: 14px }  /* ":" が無い宣言だけ捨てられ、font-size は生きる */
        @media (max-width: 600px) { p { display: none } }  /* at-rule は読み飛ばす */
        span { color: blue
    "#;
    let sheet = CssParser::parse(css);
    println!("ルール数: {}\n", sheet.rules.len());
    for (i, rule) in sheet.rules.iter().enumerate() {
        println!("[{}] {}", i + 1, rule.to_css());
        for s in &rule.selectors {
            println!("    セレクタ {:<16} 詳細度 {:?}", s.to_css(), s.specificity());
        }
        println!();
    }
}
''',
    },
    {
        "id": "step08_style",
        "title": "ステップ 8: 要素ごとの計算値を決める(カスケード・詳細度・継承)",
        "modules": HTML_ALL + CSS_ALL + ["renderer/style"],
        "main": r'''
use html::dom::{Node, NodeKind, NodeRef};
use html::parser::HtmlParser;
use css::parser::CssParser;
use css::cssom::StyleSheet;
use renderer::style::{compute_style, explain_cascade, ComputedStyle};

fn walk(node: &NodeRef, sheet: &StyleSheet, parent: Option<&ComputedStyle>, depth: usize) {
    let indent = "  ".repeat(depth);
    match &node.kind {
        NodeKind::Element(el) => {
            let style = compute_style(el, sheet, parent);
            println!(
                "{indent}<{}{}{}>  display={:?} color={} bg={:?} font={}px decoration={:?}",
                el.tag,
                el.id().map(|i| format!(" id={i}")).unwrap_or_default(),
                el.attribute("class").map(|c| format!(" class={c}")).unwrap_or_default(),
                style.display,
                style.color.hex(),
                style.background_color.map(|c| c.hex()),
                style.font_size,
                style.text_decoration,
            );
            for line in explain_cascade(el, sheet) {
                println!("{indent}      ↳ {line}");
            }
            for c in node.children() {
                walk(&c, sheet, Some(&style), depth + 1);
            }
        }
        NodeKind::Text(t) if !t.trim().is_empty() => println!("{indent}#text {:?}", t.trim()),
        _ => {
            for c in node.children() {
                walk(&c, sheet, parent, depth);
            }
        }
    }
}

fn main() {
    let html = r#"<body>
      <div id="main" class="box">
        <h1>見出し</h1>
        <p class="note">本文 <a href="/x">リンク</a> と <span>span</span></p>
      </div>
    </body>"#;
    let css = r#"
        body   { color: black; font-size: 16px }
        #main  { background-color: lightyellow; color: navy }
        .box   { color: green }          /* #main より詳細度が低いので負ける */
        p      { color: red }            /* .note より詳細度が低いので負ける */
        .note  { color: gray }
        p      { font-size: 14px }
        a      { text-decoration: none } /* UA の下線を作者が消す */
        div    { color: purple }         /* 同じ要素に #main があるので負ける */
    "#;
    let doc = HtmlParser::parse(html);
    let sheet = CssParser::parse(css);
    walk(&doc, &sheet, None, 0);
    println!("\n読み方: 各要素の下の ↳ が「当たった作者の宣言」を弱い順に並べたもの。最後の行がその property の勝者。");
    println!("        ↳ に無い値は、継承(color / font-size / text-decoration)か UA 既定(display, h1 の大きさ, a の青)から来ている。");
    let _ = Node::document; // (Node を使っていることを示すだけ)
}
''',
    },
    {
        "id": "step09_layout",
        "title": "ステップ 9: 箱の木を作り、位置と大きさを決める(通常フロー)",
        "modules": HTML_ALL + CSS_ALL + ["renderer/style", "renderer/layout"],
        "main": r'''
use html::parser::HtmlParser;
use css::parser::CssParser;
use renderer::layout::{dump_layout, layout_document, VIEWPORT_WIDTH};

fn show(title: &str, html: &str, css: &str) {
    println!("══ {title} ══");
    let doc = HtmlParser::parse(html);
    let sheet = CssParser::parse(css);
    match layout_document(&doc, &sheet) {
        Some(root) => print!("{}", dump_layout(&root)),
        None => println!("(箱が 1 つもない)"),
    }
    println!();
}

fn main() {
    println!("ビューポートの幅 = {VIEWPORT_WIDTH}px、1 文字 = font-size × 0.5 px、1 行 = font-size × 1.5 px\n");
    show("block は縦に積まれる", "<h1>Title</h1><p>one</p><p>two</p>", "");
    show("inline と文字は横に並ぶ", "<p>ab<a>cd</a>ef</p>", "");
    show("右端で折り返す(80 文字 = 640px)", &format!("<p>{}</p>", "x".repeat(100)), "");
    show("display: none は箱を作らない", "<p>a</p><p class=\"gone\">b</p><p>c</p>", ".gone { display: none }");
    show("width / height の指定", "<div>x</div><div>y</div>", "div { width: 100px; height: 40px }");
    show("空白だけの文字は箱にならない", "<div>\n  <p>a</p>\n  <p>b</p>\n</div>", "");
}
''',
    },
    {
        "id": "step10_paint",
        "title": "ステップ 10: 表示リストを作り、文字のキャンバスに描く",
        "modules": HTML_ALL + CSS_ALL + ["renderer/style", "renderer/layout", "renderer/paint"],
        "main": r'''
use html::parser::HtmlParser;
use css::parser::CssParser;
use renderer::layout::{layout_document, VIEWPORT_WIDTH};
use renderer::paint::{dump_display_list, paint, render_ascii};

fn main() {
    let html = r#"<body>
      <h1>Hello, tinybrowser</h1>
      <p>This is a paragraph with <a href="/x">a link</a> inside. The text keeps going so that it has to wrap at the right edge.</p>
      <p class="note">note: <span id="n">0</span></p>
      <p class="gone">hidden</p>
    </body>"#;
    let css = r#"
      h1    { color: navy }
      .note { background-color: lightyellow }
      #n    { color: red }
      .gone { display: none }
    "#;
    let doc = HtmlParser::parse(html);
    let root = layout_document(&doc, &CssParser::parse(css)).expect("body がある");
    let items = paint(&root);
    println!("── 表示リスト({} 個の命令) ──", items.len());
    println!("{}\n", dump_display_list(&items));
    println!("── 文字のキャンバス(1 セル = 8×24px) ──");
    println!("{}", render_ascii(&items, VIEWPORT_WIDTH));
}
''',
    },
    {
        "id": "step11_js_lexer",
        "title": "ステップ 11: JavaScript をトークンに切る",
        "modules": ["js/lexer"],
        "main": r'''
use js::lexer::JsLexer;

fn show(src: &str) {
    println!("── {src:?}");
    for t in JsLexer::new(src) {
        println!("   {t:?}");
    }
    println!();
}

fn main() {
    show("1 + 2");
    show("var x = 42;");
    show("function add(a, b) { return a + b; }");
    show("document.getElementById(\"title\").textContent = 'hi'");
    show("x = x - 1 // コメントは消える");
}
''',
    },
    {
        "id": "step12_js_parser",
        "title": "ステップ 12: トークンから抽象構文木(AST)を組み立てる",
        "modules": ["js/lexer", "js/ast", "js/parser"],
        "main": r'''
use js::parser::JsParser;

fn show(src: &str) {
    println!("── {src}");
    match JsParser::parse(src) {
        Ok(program) => {
            for node in &program.body {
                print!("{}", node.dump());
            }
        }
        Err(e) => println!("   {e}"),
    }
    println!();
}

fn main() {
    show("1 + 2 - 3");
    show("var x = 1; x = x + 1;");
    show("function add(a, b) { return a + b; } add(1, 2)");
    show("document.getElementById('t').textContent = 'hi'");
    show("var = 1");
    show("f(");
}
''',
    },
    {
        "id": "step13_js_runtime",
        "title": "ステップ 13: AST を評価する(環境・スコープ・関数)",
        "modules": ["js/lexer", "js/ast", "js/parser"] + HTML_ALL + ["js/runtime"],
        "main": r'''
use js::runtime::JsRuntime;

fn show(src: &str) {
    println!("── {src}");
    let mut rt = JsRuntime::new(None);
    match rt.run(src) {
        Ok(v) => println!("   結果: {v}"),
        Err(e) => println!("   {e}"),
    }
    for line in &rt.console_output {
        println!("   [console] {line}");
    }
    println!();
}

fn main() {
    show("1 + 2");
    show("'a' + 1 + 2");
    show("var x = 10; var y = x - 3; y");
    show("var x = 1; x = x + 1; x");
    show("function add(a, b) { return a + b; } add(1, 2)");
    show("var base = 10; function f(a) { return base + a; } f(5)");
    show("function f() { var inner = 1; return inner; } f(); inner");
    show("g(); function g() { return 7; }");
    show("console.log('合計は', 1 + 2); console.log('二行目')");
    show("nope + 1");
    show("(1)(2)");
}
''',
    },
    {
        "id": "step14_browser_api",
        "title": "ステップ 14: ブラウザ API で DOM を書き換え、描き直す",
        "modules": HTML_ALL + CSS_ALL + ["renderer/style", "renderer/layout", "renderer/paint", "js/lexer", "js/ast", "js/parser", "js/runtime", "browser"],
        "main": r'''
use browser::Page;
use html::dom::Node;
use js::runtime::JsRuntime;

const HTML: &str = r#"<html><head><style>
    h1 { color: navy }
    #count { color: red; background-color: lightyellow }
  </style></head><body>
    <h1>Counter</h1>
    <p>Clicks: <span id="count">0</span></p>
    <script>
      var n = 1 + 2;
      var el = document.getElementById("count");
      el.textContent = "" + n;
      console.log("script ran. count =", el.textContent);
    </script>
  </body></html>"#;

fn main() {
    let mut page = Page::new();
    page.load(HTML);
    for line in &page.console {
        println!("[console] {line}");
    }
    for e in &page.script_errors {
        println!("[script error] {e}");
    }
    println!("── 読み込み直後(script が count を 0 → 3 に書き換えた後) ──");
    println!("{}\n", page.ascii());

    // あとから JavaScript を実行して DOM を変え、描き直す(ボタンのクリックで走る処理の代わり)
    let doc = page.document.clone().expect("document");
    let mut rt = JsRuntime::new(Some(doc.clone()));
    rt.run("var el = document.getElementById('count'); el.textContent = el.textContent + '0'").unwrap();
    println!("── DOM を書き換えた直後(まだ描き直していない。表示は古いまま) ──");
    println!("count の textContent = {:?}", Node::get_element_by_id(&doc, "count").unwrap().text_content());
    println!("{}\n", page.ascii());

    page.relayout();
    println!("── relayout() の後 ──");
    println!("{}", page.ascii());
}
''',
    },
]


def main():
    os.makedirs(OUT, exist_ok=True)
    os.makedirs(os.path.join(OUT, "run"), exist_ok=True)
    for step in STEPS:
        for strip in (False, True):
            body = build_modules(step["modules"], strip_tests=strip)
            text = HEADER.format(title=step["title"], id=step["id"]) + body + "\n\n" + step["main"].strip("\n") + "\n"
            sub = "run" if strip else ""
            path = os.path.join(OUT, sub, step["id"] + ".rs")
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)
            print(f"wrote {os.path.relpath(path, ROOT):40} {len(text.splitlines()):5} lines {len(text.encode()):7} bytes")


if __name__ == "__main__":
    main()
