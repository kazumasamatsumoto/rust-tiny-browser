//! HTML の構文解析。トークンの列から DOM ツリーを組み立てる(HTML Standard 13.2.6 Tree construction)。
//!
//! 仕様: 「構文解析器の入力は字句解析器が出すトークンの列。出力はその Document の DOM ツリーを
//! 動的に変更・拡張したもの。」
//!
//! 2 つの状態で動く:
//! - insertion mode(挿入モード): 「構文解析の主要な動作を制御する状態変数」。
//!   仕様には 23 個あり、この実装は 9 個(initial → before html → before head → in head → after head →
//!   in body / text → after body → after after body)。
//! - stack of open elements(開いている要素のスタック): 「最初は空。最後に追加したものが一番下(= 今の要素)」。
//!
//! 仕様のどの節の処理を簡略化したかは各モードのコメントに書いた。

use crate::html::dom::{Node, NodeRef};
use crate::html::token::{Attribute, HtmlToken};
use crate::html::tokenizer::HtmlTokenizer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertionMode {
    Initial,
    BeforeHtml,
    BeforeHead,
    InHead,
    AfterHead,
    InBody,
    /// script / style / title の中身を文字として集めるモード
    Text,
    AfterBody,
    AfterAfterBody,
}

/// 終了タグを持たない要素(HTML Standard 13.1.2 の void elements)
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr",
];

/// head の中で中身を文字として読む要素
const HEAD_TEXT_ELEMENTS: &[&str] = &["script", "style", "title"];

/// 開始タグが来たとき、開いている <p> を先に閉じる要素(仕様 "in body" の address, article, ... の規則の一部)
const CLOSES_P: &[&str] = &[
    "address", "article", "aside", "blockquote", "div", "dl", "fieldset", "footer", "form", "h1", "h2", "h3",
    "h4", "h5", "h6", "header", "hr", "main", "nav", "ol", "p", "pre", "section", "table", "ul",
];

pub struct HtmlParser {
    tokenizer: HtmlTokenizer,
    mode: InsertionMode,
    /// Text モードから戻る先
    original_mode: InsertionMode,
    document: NodeRef,
    open_elements: Vec<NodeRef>,
    /// まだノードにしていない連続した文字
    text_buffer: String,
    /// モードの遷移の記録(学習用)
    trace: Vec<String>,
}

impl HtmlParser {
    pub fn new(html: &str) -> Self {
        HtmlParser {
            tokenizer: HtmlTokenizer::new(html),
            mode: InsertionMode::Initial,
            original_mode: InsertionMode::Initial,
            document: Node::document(),
            open_elements: Vec::new(),
            text_buffer: String::new(),
            trace: Vec::new(),
        }
    }

    /// HTML 文字列から Document を作る
    pub fn parse(html: &str) -> NodeRef {
        HtmlParser::new(html).run().0
    }

    /// Document と、挿入モードの遷移の記録を返す
    pub fn run(mut self) -> (NodeRef, Vec<String>) {
        self.construct_tree();
        (self.document, self.trace)
    }

    fn construct_tree(&mut self) {
        let mut token = self.tokenizer.next();
        while let Some(t) = token.take() {
            if !matches!(t, HtmlToken::Char(_)) {
                self.flush_text();
            }
            let reprocess = self.step(&t);
            if matches!(t, HtmlToken::Eof) && !reprocess {
                break;
            }
            token = if reprocess { Some(t) } else { self.tokenizer.next() };
        }
        self.flush_text();
    }

    /// 1 トークンを今のモードで処理する。true を返したら同じトークンを新しいモードでもう一度処理する
    /// (仕様の "reprocess the token")
    fn step(&mut self, token: &HtmlToken) -> bool {
        use self::HtmlToken::*;
        use self::InsertionMode::*;

        match self.mode {
            // 13.2.6.4.1 "initial": 本物は DOCTYPE を見る。この実装は DOCTYPE を字句解析で捨てているので
            // 空白を読み飛ばし、それ以外が来たら before html へ
            Initial => match token {
                Char(c) if c.is_ascii_whitespace() => false,
                _ => {
                    self.switch(BeforeHtml, token);
                    true
                }
            },

            // 13.2.6.4.2 "before html": <html> があればそれを、無ければ暗黙の <html> を作る
            BeforeHtml => match token {
                Char(c) if c.is_ascii_whitespace() => false,
                StartTag { tag, attributes, .. } if tag == "html" => {
                    self.insert_element("html", attributes.clone());
                    self.switch(BeforeHead, token);
                    false
                }
                EndTag { tag } if !["head", "body", "html", "br"].contains(&tag.as_str()) => false,
                _ => {
                    self.insert_element("html", vec![]);
                    self.switch(BeforeHead, token);
                    true
                }
            },

            // 13.2.6.4.3 "before head"
            BeforeHead => match token {
                Char(c) if c.is_ascii_whitespace() => false,
                StartTag { tag, attributes, .. } if tag == "head" => {
                    self.insert_element("head", attributes.clone());
                    self.switch(InHead, token);
                    false
                }
                EndTag { tag } if !["head", "body", "html", "br"].contains(&tag.as_str()) => false,
                _ => {
                    self.insert_element("head", vec![]);
                    self.switch(InHead, token);
                    true
                }
            },

            // 13.2.6.4.4 "in head": style / script / title は中身を文字として集める。meta / link は子を持たない
            InHead => match token {
                Char(c) if c.is_ascii_whitespace() => {
                    self.text_buffer.push(*c);
                    false
                }
                StartTag { tag, attributes, .. } if HEAD_TEXT_ELEMENTS.contains(&tag.as_str()) => {
                    self.insert_element(tag, attributes.clone());
                    self.original_mode = InHead;
                    self.switch(Text, token);
                    false
                }
                StartTag { tag, attributes, .. } if VOID_ELEMENTS.contains(&tag.as_str()) => {
                    self.insert_element(tag, attributes.clone());
                    self.open_elements.pop();
                    false
                }
                EndTag { tag } if tag == "head" => {
                    self.open_elements.pop();
                    self.switch(AfterHead, token);
                    false
                }
                EndTag { tag } if !["body", "html", "br"].contains(&tag.as_str()) => false,
                _ => {
                    // head の中に書けないものが来た = head はここで終わり
                    self.open_elements.pop();
                    self.switch(AfterHead, token);
                    true
                }
            },

            // 13.2.6.4.6 "after head": <body> があればそれを、無ければ暗黙の <body> を作る
            AfterHead => match token {
                Char(c) if c.is_ascii_whitespace() => {
                    self.text_buffer.push(*c);
                    false
                }
                StartTag { tag, attributes, .. } if tag == "body" => {
                    self.insert_element("body", attributes.clone());
                    self.switch(InBody, token);
                    false
                }
                StartTag { tag, attributes, .. } if HEAD_TEXT_ELEMENTS.contains(&tag.as_str()) => {
                    // 本物は head をスタックに戻して "in head" の規則で処理する。ここでは直接 head に入れる
                    if let Some(head) = self.find_head() {
                        let el = Node::element(tag, attributes.clone());
                        Node::append_child(&head, el.clone());
                        self.open_elements.push(el);
                        self.original_mode = AfterHead;
                        self.switch(Text, token);
                    }
                    false
                }
                EndTag { tag } if !["body", "html", "br"].contains(&tag.as_str()) => false,
                _ => {
                    self.insert_element("body", vec![]);
                    self.switch(InBody, token);
                    true
                }
            },

            // 13.2.6.4.7 "in body": 一番長い節。ここでは「要素を入れて積む」「終了タグで積んだ分を下ろす」が芯
            InBody => match token {
                Char(c) => {
                    self.text_buffer.push(*c);
                    false
                }
                StartTag { tag, attributes, .. } => {
                    match tag.as_str() {
                        "html" | "head" | "body" => return false, // 2 回目は無視(parse error)
                        _ => {}
                    }
                    // <p> が開いたまま <div> や <h1> が来たら、<p> はそこで閉じる(仕様 "close a p element")
                    if CLOSES_P.contains(&tag.as_str()) && self.has_in_scope("p") {
                        self.pop_until("p");
                    }
                    self.insert_element(tag, attributes.clone());
                    // void 要素は子を持たないので、置いた直後に下ろす。
                    // <div/> のように void 以外に付いた "/"(self_closing)は、仕様では無視される(parse error)。
                    // だから <div/>x の x は div の中に入る
                    if VOID_ELEMENTS.contains(&tag.as_str()) {
                        self.open_elements.pop();
                    }
                    false
                }
                EndTag { tag } => {
                    match tag.as_str() {
                        "body" => {
                            if self.has_in_scope("body") {
                                self.switch(AfterBody, token);
                            }
                            false
                        }
                        "html" => {
                            if self.has_in_scope("body") {
                                self.switch(AfterBody, token);
                                return true;
                            }
                            false
                        }
                        "br" => {
                            // </br> は <br> として扱う(仕様どおり)
                            self.insert_element("br", vec![]);
                            self.open_elements.pop();
                            false
                        }
                        _ => {
                            if self.has_in_scope(tag) {
                                self.pop_until(tag);
                            }
                            // 対応する開始タグが無い終了タグは無視(parse error)
                            false
                        }
                    }
                }
                Eof => false,
            },

            // 13.2.6.4.8 "text": 終了タグが来るまで文字を集める
            Text => match token {
                Char(c) => {
                    self.text_buffer.push(*c);
                    false
                }
                EndTag { .. } => {
                    self.open_elements.pop();
                    let back = self.original_mode;
                    self.switch(back, token);
                    false
                }
                _ => {
                    // EOF(閉じ忘れ): 要素を閉じて元のモードで EOF を処理し直す
                    self.open_elements.pop();
                    let back = self.original_mode;
                    self.switch(back, token);
                    true
                }
            },

            // 13.2.6.4.19 "after body"
            AfterBody => match token {
                Char(c) if c.is_ascii_whitespace() => {
                    self.text_buffer.push(*c);
                    false
                }
                EndTag { tag } if tag == "html" => {
                    self.switch(AfterAfterBody, token);
                    false
                }
                Eof => false,
                _ => {
                    // </body> の後に中身が来た(parse error): body に戻って続ける
                    self.switch(InBody, token);
                    true
                }
            },

            // 13.2.6.4.22 "after after body"
            AfterAfterBody => match token {
                Char(c) if c.is_ascii_whitespace() => {
                    self.text_buffer.push(*c);
                    false
                }
                Eof => false,
                _ => {
                    self.switch(InBody, token);
                    true
                }
            },
        }
    }

    fn switch(&mut self, to: InsertionMode, because_of: &HtmlToken) {
        self.trace
            .push(format!("{:?} → {:?}  ({})", self.mode, to, describe(because_of)));
        self.mode = to;
    }

    /// 今の要素(スタックの一番下)。空なら document
    fn current_node(&self) -> NodeRef {
        self.open_elements
            .last()
            .cloned()
            .unwrap_or_else(|| self.document.clone())
    }

    /// 要素を作って今の要素の子にし、スタックに積む(仕様 "insert an HTML element")
    fn insert_element(&mut self, tag: &str, attributes: Vec<Attribute>) -> NodeRef {
        let el = Node::element(tag, attributes);
        Node::append_child(&self.current_node(), el.clone());
        self.open_elements.push(el.clone());
        el
    }

    /// 溜めた文字を Text ノードにして今の要素に付ける(仕様 "insert a character" をまとめてやる)
    fn flush_text(&mut self) {
        if self.text_buffer.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.text_buffer);
        let parent = self.current_node();
        // document 直下に文字は置けない(before html などで空白を無視しているので通常は来ない)
        if parent.as_element().is_some() {
            Node::append_child(&parent, Node::text(&text));
        }
    }

    /// スタックにその要素があるか(仕様の "has an element in scope" を単純化したもの)
    fn has_in_scope(&self, tag: &str) -> bool {
        self.open_elements.iter().any(|n| n.is_element(tag))
    }

    /// その要素が下ろされるまでスタックを下ろす
    fn pop_until(&mut self, tag: &str) {
        while let Some(n) = self.open_elements.pop() {
            if n.is_element(tag) {
                break;
            }
        }
    }

    fn find_head(&self) -> Option<NodeRef> {
        let html = self.document.first_child()?;
        html.children().into_iter().find(|n| n.is_element("head"))
    }
}

fn describe(token: &HtmlToken) -> String {
    match token {
        HtmlToken::StartTag { tag, .. } => format!("<{tag}>"),
        HtmlToken::EndTag { tag } => format!("</{tag}>"),
        HtmlToken::Char(c) => format!("文字 {c:?}"),
        HtmlToken::Eof => "EOF".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::dom::NodeKind;

    fn tags_of(node: &NodeRef) -> Vec<String> {
        node.children()
            .iter()
            .map(|c| match &c.kind {
                NodeKind::Element(e) => e.tag.clone(),
                NodeKind::Text(t) => format!("#text({})", t.trim()),
                NodeKind::Document => "#document".into(),
            })
            .collect()
    }

    fn body_of(doc: &NodeRef) -> NodeRef {
        let html = doc.first_child().unwrap();
        html.children().into_iter().find(|n| n.is_element("body")).unwrap()
    }

    #[test]
    fn empty_input_still_makes_html_head_body() {
        let doc = HtmlParser::parse("");
        let html = doc.first_child().unwrap();
        assert_eq!(html.tag(), Some("html"));
        assert_eq!(tags_of(&html), vec!["head", "body"]);
    }

    #[test]
    fn text_only_goes_into_body() {
        let doc = HtmlParser::parse("hello");
        assert_eq!(tags_of(&body_of(&doc)), vec!["#text(hello)"]);
    }

    #[test]
    fn full_document() {
        let doc = HtmlParser::parse(
            "<html><head><title>T</title></head><body><h1 id=\"t\">Hi</h1><p>a <a href=\"/x\">link</a></p></body></html>",
        );
        let html = doc.first_child().unwrap();
        assert_eq!(tags_of(&html), vec!["head", "body"]);
        let head = html.first_child().unwrap();
        assert_eq!(tags_of(&head), vec!["title"]);
        assert_eq!(head.first_child().unwrap().text_content(), "T");
        let body = body_of(&doc);
        assert_eq!(tags_of(&body), vec!["h1", "p"]);
        let p = body.last_child().unwrap();
        assert_eq!(tags_of(&p), vec!["#text(a)", "a"]);
        assert_eq!(Node::get_element_by_id(&doc, "t").unwrap().text_content(), "Hi");
    }

    #[test]
    fn missing_end_tags_are_repaired() {
        // <p> の閉じ忘れ: 次の <p> が来た時点で前の <p> は閉じる(兄弟になる。入れ子にならない)
        let doc = HtmlParser::parse("<p>one<p>two");
        assert_eq!(tags_of(&body_of(&doc)), vec!["p", "p"]);
        // <div> の中の <p> の閉じ忘れ: </div> で p も閉じる
        let doc = HtmlParser::parse("<div><p>x</div><span>y</span>");
        assert_eq!(tags_of(&body_of(&doc)), vec!["div", "span"]);
    }

    #[test]
    fn self_closing_slash_is_ignored_on_non_void_elements() {
        // <br/> は void なので子を持たない。<div/> の "/" は無視されて div は開いたまま(仕様どおり)
        let doc = HtmlParser::parse("<p>a<br/>b</p><div/>x");
        let body = body_of(&doc);
        assert_eq!(tags_of(&body), vec!["p", "div"]);
        assert_eq!(body.last_child().unwrap().text_content(), "x");
    }

    #[test]
    fn stray_end_tag_is_ignored() {
        let doc = HtmlParser::parse("<p>a</b>b</p>");
        let p = body_of(&doc).first_child().unwrap();
        assert_eq!(p.text_content(), "ab");
    }

    #[test]
    fn style_and_script_keep_raw_text() {
        let doc = HtmlParser::parse("<style>p > a { color: red }</style><script>if (1 < 2) {}</script><p>x</p>");
        let styles = Node::find_all(&doc, "style");
        assert_eq!(styles[0].text_content(), "p > a { color: red }");
        let scripts = Node::find_all(&doc, "script");
        assert_eq!(scripts[0].text_content(), "if (1 < 2) {}");
        assert_eq!(tags_of(&body_of(&doc)), vec!["p"]);
    }

    #[test]
    fn mode_trace_is_recorded() {
        let (_, trace) = HtmlParser::new("<p>x</p>").run();
        assert_eq!(trace[0], "Initial → BeforeHtml  (<p>)");
        assert!(trace.iter().any(|t| t.contains("AfterHead → InBody")));
    }
}
