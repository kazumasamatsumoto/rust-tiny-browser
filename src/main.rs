//! コマンドラインの入口。
//!
//! ```text
//! tinybrowser http://localhost:8000/index.html      # HTTP で取ってきて描く
//! tinybrowser --file page.html                       # ローカルのファイルを描く
//! 追加オプション: --dom(DOM を表示) --tokens(HTML のトークンを表示) --layout(箱の木を表示)
//!                 --display-list(表示リストを表示) --headers(HTTP のヘッダーを表示)
//! ```

use std::process::ExitCode;

use tinybrowser::browser::Page;
use tinybrowser::html::dom::Node;
use tinybrowser::html::tokenizer::HtmlTokenizer;
use tinybrowser::renderer::layout::dump_layout;
use tinybrowser::renderer::paint::dump_display_list;
use tinybrowser::{http, url};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flags: Vec<&str> = args.iter().filter(|a| a.starts_with("--")).map(|s| s.as_str()).collect();
    let positional: Vec<&str> = args.iter().filter(|a| !a.starts_with("--")).map(|s| s.as_str()).collect();

    let html = if flags.contains(&"--file") {
        let Some(path) = positional.first() else {
            eprintln!("使い方: tinybrowser --file <path.html>");
            return ExitCode::from(2);
        };
        match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("読めません {path}: {e}");
                return ExitCode::from(1);
            }
        }
    } else {
        let Some(target) = positional.first() else {
            eprintln!("使い方: tinybrowser <http://host[:port]/path> | --file <path.html>");
            return ExitCode::from(2);
        };
        let parsed = match url::parse(target) {
            Ok(u) => u,
            Err(e) => {
                eprintln!("URL が不正です: {e}");
                return ExitCode::from(1);
            }
        };
        println!("GET {}  (host={} port={} path={})", parsed, parsed.host, parsed.port, parsed.request_target());
        let response = match http::fetch(&parsed) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        };
        println!("{} {} {}", response.version, response.status_code, response.reason_phrase);
        if flags.contains(&"--headers") {
            for h in &response.headers {
                println!("  {}: {}", h.name, h.value);
            }
        }
        response.body
    };

    if flags.contains(&"--tokens") {
        println!("── HTML トークン ──");
        for t in HtmlTokenizer::new(&html) {
            println!("{t:?}");
        }
    }

    let mut page = Page::new();
    page.load(&html);

    if flags.contains(&"--dom") {
        println!("── DOM ──");
        if let Some(doc) = &page.document {
            print!("{}", Node::dump(doc));
        }
    }
    if flags.contains(&"--layout") {
        println!("── 箱の木 ──");
        if let Some(root) = &page.layout_root {
            print!("{}", dump_layout(root));
        }
    }
    if flags.contains(&"--display-list") {
        println!("── 表示リスト ──");
        println!("{}", dump_display_list(&page.display_list));
    }
    for line in &page.console {
        println!("[console] {line}");
    }
    for e in &page.script_errors {
        eprintln!("[script error] {e}");
    }
    println!("{}", page.ascii());
    ExitCode::SUCCESS
}
