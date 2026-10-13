//! 部品を 1 ページ分つなぐ。
//!
//! HTML を受け取ってから画面に出るまで:
//! 1. HTML → DOM(html::parser)
//! 2. DOM の <style> の中身 → CSSOM(css::parser)
//! 3. DOM の <script> の中身 → 実行(js::runtime)。DOM が書き換わるかもしれない
//! 4. DOM + CSSOM → 箱の木と座標(renderer::layout)
//! 5. 箱の木 → 表示リスト(renderer::paint)
//!
//! 本物との違い: 本物は <script> に出会った時点で構文解析を止めて実行する(その後の HTML はまだ無い)。
//! この実装は DOM を全部作ってから実行するので、`defer` 付きの script に近い。

use crate::css::cssom::StyleSheet;
use crate::css::parser::CssParser;
use crate::html::dom::{Node, NodeRef};
use crate::html::parser::HtmlParser;
use crate::js::runtime::JsRuntime;
use crate::renderer::layout::{layout_document, LayoutBox, VIEWPORT_WIDTH};
use crate::renderer::paint::{paint, render_ascii, DisplayItem};

#[derive(Default)]
pub struct Page {
    pub document: Option<NodeRef>,
    pub stylesheet: StyleSheet,
    pub layout_root: Option<LayoutBox>,
    pub display_list: Vec<DisplayItem>,
    /// script の console.log の出力
    pub console: Vec<String>,
    /// script の実行エラー(あっても描画は続ける。本物と同じ)
    pub script_errors: Vec<String>,
}

impl Page {
    pub fn new() -> Self {
        Page::default()
    }

    /// HTML 文字列を読み込んで、描画の準備まで済ませる
    pub fn load(&mut self, html: &str) {
        let document = HtmlParser::parse(html);
        self.stylesheet = collect_stylesheet(&document);
        self.document = Some(document);
        self.run_scripts();
        self.relayout();
    }

    /// DOM と CSSOM から箱の木と表示リストを作り直す
    pub fn relayout(&mut self) {
        let Some(doc) = &self.document else { return };
        self.layout_root = layout_document(doc, &self.stylesheet);
        self.display_list = self.layout_root.as_ref().map(paint).unwrap_or_default();
    }

    fn run_scripts(&mut self) {
        let Some(doc) = &self.document else { return };
        for script in Node::find_all(doc, "script") {
            let source = script.text_content();
            let mut rt = JsRuntime::new(Some(doc.clone()));
            if let Err(e) = rt.run(&source) {
                self.script_errors.push(e.to_string());
            }
            self.console.extend(rt.console_output);
        }
    }

    /// 文字のキャンバスに描いたもの
    pub fn ascii(&self) -> String {
        render_ascii(&self.display_list, VIEWPORT_WIDTH)
    }
}

/// DOM の中の <style> を全部集めて 1 つの StyleSheet にする(出現順を保つ)
pub fn collect_stylesheet(document: &NodeRef) -> StyleSheet {
    let mut sheet = StyleSheet::default();
    for style in Node::find_all(document, "style") {
        let parsed = CssParser::parse(&style.text_content());
        sheet.rules.extend(parsed.rules);
    }
    sheet
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"<html><head><style>
      h1 { color: navy }
      #msg { background-color: yellow }
    </style></head><body>
      <h1>Title</h1>
      <p id="msg">before</p>
      <script>
        var el = document.getElementById("msg");
        el.textContent = "after";
        console.log("done:", el.textContent);
      </script>
    </body></html>"#;

    #[test]
    fn whole_pipeline() {
        let mut page = Page::new();
        page.load(HTML);
        assert_eq!(page.stylesheet.rules.len(), 2);
        assert_eq!(page.console, vec!["done: after"]);
        assert!(page.script_errors.is_empty());
        let ascii = page.ascii();
        assert!(ascii.contains("Title"));
        assert!(ascii.contains("after"));
        assert!(!ascii.contains("before"));
        assert!(ascii.contains('░')); // 黄色の背景
    }

    #[test]
    fn script_error_does_not_stop_rendering() {
        let mut page = Page::new();
        page.load("<p>x</p><script>undefinedThing.foo</script>");
        assert_eq!(page.script_errors.len(), 1);
        assert!(page.ascii().contains('x'));
    }
}
