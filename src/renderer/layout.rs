//! レイアウト。DOM + 計算値から「箱の木」を作り、各箱の位置と大きさを決める。
//!
//! CSS Display Level 3 / CSS 2 の visual formatting model のうち、通常フロー(normal flow)だけ:
//! - block の箱は縦に積まれ、幅は親いっぱい、高さは中身で決まる
//! - inline の箱と文字は横に並び、右端で折り返す
//! - display: none の要素は箱を作らない(子孫ごと)
//!
//! 文字の大きさは等幅フォントを仮定し、1 文字の幅 = font-size × 0.5、行の高さ = font-size × 1.5 とする。
//! 単語の途中で折り返す(単語境界は見ない)。margin / padding / border / float / flex は無い。

use crate::css::cssom::StyleSheet;
use crate::html::dom::{NodeKind, NodeRef};
use crate::renderer::style::{compute_style, ComputedStyle, Display};

/// ビューポートの幅(px)。本物はウィンドウの幅
pub const VIEWPORT_WIDTH: f32 = 640.0;
/// 1 文字の幅 = font-size × この比(等幅フォントの近似)
pub const CHAR_WIDTH_RATIO: f32 = 0.5;
/// 行の高さ = font-size × この比
pub const LINE_HEIGHT_RATIO: f32 = 1.5;

#[derive(Debug, Clone, PartialEq)]
pub enum BoxKind {
    Block,
    Inline,
    /// 文字列。空白は 1 つにまとめてある
    Text(String),
}

/// 折り返し後の 1 行分の文字列と位置
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub text: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBox {
    pub kind: BoxKind,
    /// 要素名。文字列の箱は "#text"
    pub tag: String,
    pub style: ComputedStyle,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Text の箱だけが持つ、折り返し後の行
    pub lines: Vec<TextLine>,
    pub children: Vec<LayoutBox>,
}

impl LayoutBox {
    fn new(kind: BoxKind, tag: &str, style: ComputedStyle) -> Self {
        LayoutBox {
            kind,
            tag: tag.to_string(),
            style,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            lines: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn is_text(&self) -> bool {
        matches!(self.kind, BoxKind::Text(_))
    }
}

/// DOM のノードから箱の木を作る(仕様で言う box tree の生成)。display: none なら None
pub fn build_layout_tree(node: &NodeRef, sheet: &StyleSheet, parent_style: Option<&ComputedStyle>) -> Option<LayoutBox> {
    match &node.kind {
        NodeKind::Document => {
            // document 自身は箱を作らない。最初に箱になった子(html)を返す
            node.children()
                .iter()
                .find_map(|c| build_layout_tree(c, sheet, parent_style))
        }
        NodeKind::Element(el) => {
            let style = compute_style(el, sheet, parent_style);
            let kind = match style.display {
                Display::None => return None,
                Display::Block => BoxKind::Block,
                Display::Inline => BoxKind::Inline,
            };
            let mut b = LayoutBox::new(kind, &el.tag, style);
            for child in node.children() {
                if let Some(cb) = build_layout_tree(&child, sheet, Some(&b.style)) {
                    b.children.push(cb);
                }
            }
            Some(b)
        }
        NodeKind::Text(text) => {
            // 連続する空白は 1 つに(CSS Text の white-space: normal の折りたたみ)。空白だけなら箱にしない
            let collapsed = collapse_whitespace(text);
            if collapsed.trim().is_empty() {
                return None;
            }
            let style = parent_style.cloned().unwrap_or_default();
            Some(LayoutBox::new(BoxKind::Text(collapsed), "#text", style))
        }
    }
}

fn collapse_whitespace(text: &str) -> String {
    let mut out = String::new();
    let mut in_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// 行の中の「今どこまで埋まったか」
struct Cursor {
    x: f32,
    y: f32,
    /// 行の左端(折り返し先)
    left: f32,
    /// 行の右端(ここを超えたら折り返す)
    right: f32,
    /// 今の行の高さ(行の中で一番大きい文字で決まる)
    line_height: f32,
}

impl Cursor {
    fn newline(&mut self) {
        self.y += self.line_height;
        self.x = self.left;
        self.line_height = 0.0;
    }
}

/// 木の根(block)から位置と大きさを決める。x, y は根の左上、container_width は使える幅
pub fn layout(root: &mut LayoutBox, x: f32, y: f32, container_width: f32) {
    layout_block(root, x, y, container_width);
}

fn layout_block(b: &mut LayoutBox, x: f32, y: f32, container_width: f32) {
    b.x = x;
    b.y = y;
    // 幅: 指定があればそれ、無ければ親いっぱい(auto)
    b.width = b.style.width.unwrap_or(container_width);
    let width = b.width;
    let mut cur = Cursor {
        x,
        y,
        left: x,
        right: x + width,
        line_height: 0.0,
    };
    layout_flow(&mut b.children, &mut cur, width);
    if cur.line_height > 0.0 {
        cur.newline(); // 最後の行を確定する
    }
    // 高さ: 指定があればそれ、無ければ中身の分(auto)
    b.height = b.style.height.unwrap_or(cur.y - y);
}

/// 子を順に置く。block は行を改めて縦に、inline と文字は行の中に横に
fn layout_flow(children: &mut [LayoutBox], cur: &mut Cursor, container_width: f32) {
    for child in children.iter_mut() {
        match child.kind.clone() {
            BoxKind::Block => {
                if cur.line_height > 0.0 {
                    cur.newline();
                }
                layout_block(child, cur.left, cur.y, container_width);
                cur.y = child.y + child.height;
                cur.x = cur.left;
                cur.line_height = 0.0;
            }
            BoxKind::Inline => {
                child.x = cur.x;
                child.y = cur.y;
                let start_y = cur.y;
                layout_flow(&mut child.children, cur, container_width);
                // inline の箱の大きさは「子が占めた範囲」の近似
                child.width = if cur.y == start_y { cur.x - child.x } else { container_width };
                child.height = (cur.y + cur.line_height) - start_y;
            }
            BoxKind::Text(text) => {
                let font = child.style.font_size;
                let char_w = font * CHAR_WIDTH_RATIO;
                let line_h = font * LINE_HEIGHT_RATIO;
                child.x = cur.x;
                child.y = cur.y;
                let chars: Vec<char> = text.chars().collect();
                let mut rest: &[char] = &chars;
                let mut lines = Vec::new();
                while !rest.is_empty() {
                    let mut avail = ((cur.right - cur.x) / char_w).floor() as usize;
                    if avail == 0 {
                        if cur.x > cur.left {
                            cur.newline(); // 行末に来たので折り返す
                            continue;
                        }
                        avail = 1; // 行頭でも 1 文字入らないなら、はみ出してでも 1 文字置く
                    }
                    let take = avail.min(rest.len());
                    let (head, tail) = rest.split_at(take);
                    lines.push(TextLine {
                        text: head.iter().collect(),
                        x: cur.x,
                        y: cur.y,
                    });
                    cur.x += take as f32 * char_w;
                    cur.line_height = cur.line_height.max(line_h);
                    rest = tail;
                    if !rest.is_empty() {
                        cur.newline();
                    }
                }
                child.lines = lines;
                child.width = (text.chars().count() as f32 * char_w).min(container_width);
                child.height = (cur.y + cur.line_height) - child.y;
            }
        }
    }
}

/// 箱の木を字下げした文字列にする(学習用)
pub fn dump_layout(b: &LayoutBox) -> String {
    let mut out = String::new();
    dump_into(b, 0, &mut out);
    out
}

fn dump_into(b: &LayoutBox, depth: usize, out: &mut String) {
    use std::fmt::Write as _;
    let indent = "  ".repeat(depth);
    let kind = match &b.kind {
        BoxKind::Block => "block".to_string(),
        BoxKind::Inline => "inline".to_string(),
        BoxKind::Text(t) => format!("text {:?}", t),
    };
    writeln!(
        out,
        "{indent}{kind:<12} <{}>  x={:.0} y={:.0} w={:.0} h={:.0}  font={:.0}px color={}{}",
        b.tag,
        b.x,
        b.y,
        b.width,
        b.height,
        b.style.font_size,
        b.style.color.hex(),
        b.style
            .background_color
            .map(|c| format!(" bg={}", c.hex()))
            .unwrap_or_default()
    )
    .unwrap();
    for c in &b.children {
        dump_into(c, depth + 1, out);
    }
}

/// Document から body の箱の木を作ってレイアウトまで済ませる(便利関数)
pub fn layout_document(document: &NodeRef, sheet: &StyleSheet) -> Option<LayoutBox> {
    let html = document.first_child()?;
    let body = html.children().into_iter().find(|n| n.is_element("body"))?;
    let html_style = compute_style(html.as_element()?, sheet, None);
    let mut root = build_layout_tree(&body, sheet, Some(&html_style))?;
    layout(&mut root, 0.0, 0.0, VIEWPORT_WIDTH);
    Some(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::parser::CssParser;
    use crate::html::parser::HtmlParser;

    fn lay(html: &str, css: &str) -> LayoutBox {
        let doc = HtmlParser::parse(html);
        let sheet = CssParser::parse(css);
        layout_document(&doc, &sheet).expect("body がある")
    }

    #[test]
    fn empty_body_has_zero_height() {
        let b = lay("<body></body>", "");
        assert_eq!(b.tag, "body");
        assert_eq!((b.width, b.height), (VIEWPORT_WIDTH, 0.0));
    }

    #[test]
    fn blocks_stack_vertically() {
        let b = lay("<p>a</p><p>b</p>", "");
        assert_eq!(b.children.len(), 2);
        let (p1, p2) = (&b.children[0], &b.children[1]);
        assert_eq!(p1.y, 0.0);
        assert_eq!(p1.height, 24.0); // 16px × 1.5
        assert_eq!(p2.y, 24.0);
        assert_eq!(b.height, 48.0);
    }

    #[test]
    fn inline_flows_horizontally() {
        let b = lay("<p>ab<a>cd</a></p>", "");
        let p = &b.children[0];
        let text = &p.children[0];
        let a = &p.children[1];
        assert_eq!(text.lines[0].x, 0.0);
        assert_eq!(a.x, 16.0); // "ab" = 2 文字 × 8px
        assert_eq!(p.height, 24.0); // 1 行
    }

    #[test]
    fn display_none_makes_no_box() {
        let b = lay("<p>a</p><p class=\"hide\">b</p>", ".hide { display: none }");
        assert_eq!(b.children.len(), 1);
        // body 自身が none なら箱の木は空
        let doc = HtmlParser::parse("<p>a</p>");
        let sheet = CssParser::parse("body { display: none }");
        assert!(layout_document(&doc, &sheet).is_none());
    }

    #[test]
    fn long_text_wraps() {
        // 640px ÷ 8px = 80 文字で折り返す
        let long = "x".repeat(100);
        let b = lay(&format!("<p>{long}</p>"), "");
        let text = &b.children[0].children[0];
        assert_eq!(text.lines.len(), 2);
        assert_eq!(text.lines[0].text.len(), 80);
        assert_eq!(text.lines[1].text.len(), 20);
        assert_eq!(text.lines[1].y, 24.0);
        assert_eq!(b.children[0].height, 48.0);
    }

    #[test]
    fn heading_is_taller() {
        let b = lay("<h1>T</h1>", "");
        assert_eq!(b.children[0].height, 48.0); // 32px × 1.5
    }

    #[test]
    fn explicit_width_and_height() {
        let b = lay("<div>x</div>", "div { width: 100px; height: 50px }");
        assert_eq!((b.children[0].width, b.children[0].height), (100.0, 50.0));
    }
}
