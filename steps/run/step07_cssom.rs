// ステップ 7: トークンから CSSOM(ルール・セレクタ・宣言)を組み立てる
//
// このファイル 1 つで動きます:
//   実行:      rustc --edition 2021 step07_cssom.rs && ./step07_cssom
//   テスト:    rustc --edition 2021 --test step07_cssom.rs -o step07_cssom_test && ./step07_cssom_test
// cargo のプロジェクトでは、各 `mod` の中身がそのまま src/ 以下のファイルになります。
#![allow(dead_code)] // 部品を全部 1 ファイルに入れているので、このステップで使わない関数もある

mod html {
    pub mod token {
        //! HTML のトークン。字句解析の出力であり、構文解析の入力。
        //!
        //! HTML Standard の字句解析器が出すトークンは DOCTYPE・start tag・end tag・comment・character・end-of-file の
        //! 6 種類。この実装は DOCTYPE と comment を読み捨てるので 4 種類になる。

        /// 開始タグに付く属性。`<a href="/x">` なら name = "href", value = "/x"
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Attribute {
            pub name: String,
            pub value: String,
        }

        impl Attribute {
            pub fn new(name: &str, value: &str) -> Self {
                Attribute {
                    name: name.to_string(),
                    value: value.to_string(),
                }
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum HtmlToken {
            /// `<p class="x">` や `<br/>`
            StartTag {
                tag: String,
                self_closing: bool,
                attributes: Vec<Attribute>,
            },
            /// `</p>`
            EndTag { tag: String },
            /// タグの外の 1 文字。文字列ではなく 1 文字ずつ出すのが仕様どおり
            Char(char),
            /// 入力の終わり
            Eof,
        }
    }
    pub mod dom {
        //! DOM ツリー。構文解析の出力で、CSS・レイアウト・JavaScript が読み書きする土台。
        //!
        //! DOM Standard 4.2 Node tree: ノードは parent(親)と children(子の順序付き集合)を持つ。
        //! この実装は Document / Element / Text の 3 種類だけ(Comment・DocumentType は持たない)。
        //!
        //! Rust でツリーを持つときの定番:
        //! - 子は `Rc<Node>`(所有者が複数になりうる: 親と、JavaScript から掴んだ参照)
        //! - 親は `Weak<Node>`(親→子→親 の循環参照でメモリが解放されなくなるのを避ける)
        //! - 子のリストは `RefCell` で包み、`&self` のまま付け外しできるようにする(内部可変性)

        use std::cell::RefCell;
        use std::fmt::Write as _;
        use std::rc::{Rc, Weak};

        use crate::html::token::Attribute;

        pub type NodeRef = Rc<Node>;

        /// 要素ノードの中身。
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Element {
            pub tag: String,
            pub attributes: Vec<Attribute>,
        }

        impl Element {
            pub fn attribute(&self, name: &str) -> Option<&str> {
                self.attributes
                    .iter()
                    .find(|a| a.name == name)
                    .map(|a| a.value.as_str())
            }

            pub fn id(&self) -> Option<&str> {
                self.attribute("id")
            }

            /// class 属性を空白で区切ったもの
            pub fn classes(&self) -> Vec<&str> {
                self.attribute("class")
                    .map(|c| c.split_whitespace().collect())
                    .unwrap_or_default()
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum NodeKind {
            Document,
            Element(Element),
            Text(String),
        }

        #[derive(Debug)]
        pub struct Node {
            pub kind: NodeKind,
            parent: RefCell<Weak<Node>>,
            children: RefCell<Vec<NodeRef>>,
        }

        impl Node {
            pub fn new(kind: NodeKind) -> NodeRef {
                Rc::new(Node {
                    kind,
                    parent: RefCell::new(Weak::new()),
                    children: RefCell::new(Vec::new()),
                })
            }

            pub fn document() -> NodeRef {
                Node::new(NodeKind::Document)
            }

            pub fn element(tag: &str, attributes: Vec<Attribute>) -> NodeRef {
                Node::new(NodeKind::Element(Element {
                    tag: tag.to_string(),
                    attributes,
                }))
            }

            pub fn text(data: &str) -> NodeRef {
                Node::new(NodeKind::Text(data.to_string()))
            }

            /// 末尾に子を追加する(DOM の appendChild)
            pub fn append_child(parent: &NodeRef, child: NodeRef) {
                *child.parent.borrow_mut() = Rc::downgrade(parent);
                parent.children.borrow_mut().push(child);
            }

            /// 子を全部外す
            pub fn remove_children(parent: &NodeRef) {
                for c in parent.children.borrow().iter() {
                    *c.parent.borrow_mut() = Weak::new();
                }
                parent.children.borrow_mut().clear();
            }

            pub fn parent(&self) -> Option<NodeRef> {
                self.parent.borrow().upgrade()
            }

            /// 子のリストの複製(Rc なので中身は共有される)
            pub fn children(&self) -> Vec<NodeRef> {
                self.children.borrow().clone()
            }

            pub fn first_child(&self) -> Option<NodeRef> {
                self.children.borrow().first().cloned()
            }

            pub fn last_child(&self) -> Option<NodeRef> {
                self.children.borrow().last().cloned()
            }

            pub fn as_element(&self) -> Option<&Element> {
                match &self.kind {
                    NodeKind::Element(e) => Some(e),
                    _ => None,
                }
            }

            pub fn tag(&self) -> Option<&str> {
                self.as_element().map(|e| e.tag.as_str())
            }

            pub fn is_element(&self, tag: &str) -> bool {
                self.tag() == Some(tag)
            }

            pub fn text_data(&self) -> Option<&str> {
                match &self.kind {
                    NodeKind::Text(t) => Some(t.as_str()),
                    _ => None,
                }
            }

            /// 子孫のテキストをつないだもの(DOM の textContent の getter)
            pub fn text_content(&self) -> String {
                match &self.kind {
                    NodeKind::Text(t) => t.clone(),
                    NodeKind::Document => String::new(),
                    NodeKind::Element(_) => self.children.borrow().iter().map(|c| c.text_content()).collect(),
                }
            }

            /// textContent の setter。DOM Standard の "string replace all":
            /// 子を全部外し、空でなければ新しい Text ノードを 1 つ付ける
            pub fn set_text_content(node: &NodeRef, value: &str) {
                Node::remove_children(node);
                if !value.is_empty() {
                    Node::append_child(node, Node::text(value));
                }
            }

            /// DOM Standard: 「子孫のうち tree order で最初の、ID が elementId である要素。無ければ null」
            pub fn get_element_by_id(root: &NodeRef, id: &str) -> Option<NodeRef> {
                for child in root.children.borrow().iter() {
                    if child.as_element().and_then(|e| e.id()) == Some(id) {
                        return Some(child.clone());
                    }
                    if let Some(found) = Node::get_element_by_id(child, id) {
                        return Some(found);
                    }
                }
                None
            }

            /// タグ名で子孫を集める(tree order)
            pub fn find_all(root: &NodeRef, tag: &str) -> Vec<NodeRef> {
                let mut out = Vec::new();
                Node::collect(root, tag, &mut out);
                out
            }

            fn collect(node: &NodeRef, tag: &str, out: &mut Vec<NodeRef>) {
                for child in node.children.borrow().iter() {
                    if child.is_element(tag) {
                        out.push(child.clone());
                    }
                    Node::collect(child, tag, out);
                }
            }

            /// ツリーを字下げした文字列にする(学習用・デバッグ用)
            pub fn dump(node: &NodeRef) -> String {
                let mut out = String::new();
                Node::dump_into(node, 0, &mut out);
                out
            }

            fn dump_into(node: &NodeRef, depth: usize, out: &mut String) {
                let indent = "  ".repeat(depth);
                match &node.kind {
                    NodeKind::Document => writeln!(out, "{indent}#document").unwrap(),
                    NodeKind::Element(e) => {
                        let attrs: String = e
                            .attributes
                            .iter()
                            .map(|a| format!(" {}=\"{}\"", a.name, a.value))
                            .collect();
                        writeln!(out, "{indent}<{}{}>", e.tag, attrs).unwrap();
                    }
                    NodeKind::Text(t) => {
                        if t.trim().is_empty() {
                            writeln!(out, "{indent}#text (空白のみ)").unwrap();
                        } else {
                            writeln!(out, "{indent}#text {:?}", t).unwrap();
                        }
                    }
                }
                for c in node.children.borrow().iter() {
                    Node::dump_into(c, depth + 1, out);
                }
            }
        }

        /// テストで「同じ形のツリーか」を比べるための等値。親は比べない(子から辿れるので)
        impl PartialEq for Node {
            fn eq(&self, other: &Self) -> bool {
                self.kind == other.kind && *self.children.borrow() == *other.children.borrow()
            }
        }

        impl Eq for Node {}
    }
}

mod css {
    pub mod token {
        //! CSS のトークン。
        //!
        //! CSS Syntax Module Level 3 §4 は 24 種類のトークンを定める。この実装はそのうち
        //! 使うもの(下の列挙)だけを持ち、<bad-string-token> <url-token> <CDO-token> <CDC-token> などは省く。

        #[derive(Debug, Clone, PartialEq)]
        pub enum CssToken {
            /// <ident-token>  例: color, block, red
            Ident(String),
            /// <function-token>  例: rgb(  … 名前の直後に "(" が続いたもの
            Function(String),
            /// <at-keyword-token>  例: @media
            AtKeyword(String),
            /// <hash-token>  例: #main, #ff0000(セレクタの ID と色の両方に出る)
            Hash(String),
            /// <string-token>  例: "Noto Sans"(引用符は含まない)
            Str(String),
            /// <number-token>  例: 1.5
            Number(f64),
            /// <percentage-token>  例: 50%
            Percentage(f64),
            /// <dimension-token>  例: 16px → (16.0, "px")
            Dimension(f64, String),
            /// <delim-token>  他のどれでもない 1 文字。例: . * > +
            Delim(char),
            /// <whitespace-token>  連続する空白は 1 つにまとまる
            Whitespace,
            /// <colon-token> :
            Colon,
            /// <semicolon-token> ;
            Semicolon,
            /// <comma-token> ,
            Comma,
            /// <{-token>
            OpenCurly,
            /// <}-token>
            CloseCurly,
            /// <(-token>
            OpenParen,
            /// <)-token>
            CloseParen,
            /// <[-token>
            OpenSquare,
            /// <]-token>
            CloseSquare,
        }

        impl CssToken {
            /// 人が読める形(デバッグ・学習用)。元の CSS を完全に復元するものではない
            pub fn to_css(&self) -> String {
                match self {
                    CssToken::Ident(s) => s.clone(),
                    CssToken::Function(s) => format!("{s}("),
                    CssToken::AtKeyword(s) => format!("@{s}"),
                    CssToken::Hash(s) => format!("#{s}"),
                    CssToken::Str(s) => format!("\"{s}\""),
                    CssToken::Number(n) => format_number(*n),
                    CssToken::Percentage(n) => format!("{}%", format_number(*n)),
                    CssToken::Dimension(n, u) => format!("{}{u}", format_number(*n)),
                    CssToken::Delim(c) => c.to_string(),
                    CssToken::Whitespace => " ".to_string(),
                    CssToken::Colon => ":".to_string(),
                    CssToken::Semicolon => ";".to_string(),
                    CssToken::Comma => ",".to_string(),
                    CssToken::OpenCurly => "{".to_string(),
                    CssToken::CloseCurly => "}".to_string(),
                    CssToken::OpenParen => "(".to_string(),
                    CssToken::CloseParen => ")".to_string(),
                    CssToken::OpenSquare => "[".to_string(),
                    CssToken::CloseSquare => "]".to_string(),
                }
            }
        }

        fn format_number(n: f64) -> String {
            if n.fract() == 0.0 {
                format!("{}", n as i64)
            } else {
                format!("{n}")
            }
        }
    }
    pub mod tokenizer {
        //! CSS の字句解析(CSS Syntax Module Level 3 §4 Tokenization)。
        //!
        //! HTML の字句解析と違って状態機械ではなく、「先頭の 1〜3 文字を見て、どの種類のトークンかを決めてから
        //! 必要なだけ読む」形(仕様の "consume a token")。コメント /* */ はトークンにならず、読み捨てる。

        use crate::css::token::CssToken;

        pub struct CssTokenizer {
            chars: Vec<char>,
            pos: usize,
        }

        impl CssTokenizer {
            pub fn new(css: &str) -> Self {
                CssTokenizer {
                    chars: css.chars().collect(),
                    pos: 0,
                }
            }

            fn peek(&self, offset: usize) -> Option<char> {
                self.chars.get(self.pos + offset).copied()
            }

            fn bump(&mut self) -> Option<char> {
                let c = self.peek(0);
                if c.is_some() {
                    self.pos += 1;
                }
                c
            }

            /// コメントを読み捨てる(仕様 "consume comments")
            fn skip_comments(&mut self) {
                while self.peek(0) == Some('/') && self.peek(1) == Some('*') {
                    self.pos += 2;
                    loop {
                        match self.bump() {
                            None => return,
                            Some('*') if self.peek(0) == Some('/') => {
                                self.pos += 1;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            /// 仕様 "consume a name": name code point(英数字・_・-・非 ASCII)が続く限り読む
            fn consume_name(&mut self) -> String {
                let mut s = String::new();
                while let Some(c) = self.peek(0) {
                    if is_name_char(c) {
                        s.push(c);
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                s
            }

            /// 仕様 "consume a string token": 同じ引用符が来るまで。改行が来たら打ち切る
            fn consume_string(&mut self, quote: char) -> CssToken {
                let mut s = String::new();
                while let Some(c) = self.bump() {
                    match c {
                        c if c == quote => break,
                        '\n' => break, // 本物は <bad-string-token>。ここでは打ち切って文字列にする
                        '\\' => {
                            if let Some(next) = self.bump() {
                                s.push(next);
                            }
                        }
                        _ => s.push(c),
                    }
                }
                CssToken::Str(s)
            }

            /// 仕様 "consume a numeric token": 数を読み、直後が % なら percentage、名前なら dimension
            fn consume_numeric(&mut self) -> CssToken {
                let mut text = String::new();
                if matches!(self.peek(0), Some('+') | Some('-')) {
                    text.push(self.bump().unwrap());
                }
                while let Some(c) = self.peek(0) {
                    if c.is_ascii_digit() {
                        text.push(c);
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
                    text.push('.');
                    self.pos += 1;
                    while let Some(c) = self.peek(0) {
                        if c.is_ascii_digit() {
                            text.push(c);
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                }
                let value: f64 = text.parse().unwrap_or(0.0);
                if self.peek(0) == Some('%') {
                    self.pos += 1;
                    CssToken::Percentage(value)
                } else if self.starts_name() {
                    let unit = self.consume_name();
                    CssToken::Dimension(value, unit)
                } else {
                    CssToken::Number(value)
                }
            }

            /// 次の位置から識別子が始まるか(仕様 "would start an identifier" の簡略版)
            fn starts_name(&self) -> bool {
                match self.peek(0) {
                    Some('-') => self.peek(1).is_some_and(|c| is_name_start(c) || c == '-'),
                    Some(c) => is_name_start(c),
                    None => false,
                }
            }

            /// 次の位置から数が始まるか(仕様 "would start a number")
            fn starts_number(&self) -> bool {
                match self.peek(0) {
                    Some(c) if c.is_ascii_digit() => true,
                    Some('+') | Some('-') => match self.peek(1) {
                        Some(c) if c.is_ascii_digit() => true,
                        Some('.') => self.peek(2).is_some_and(|c| c.is_ascii_digit()),
                        _ => false,
                    },
                    Some('.') => self.peek(1).is_some_and(|c| c.is_ascii_digit()),
                    _ => false,
                }
            }
        }

        fn is_name_start(c: char) -> bool {
            c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
        }

        fn is_name_char(c: char) -> bool {
            is_name_start(c) || c.is_ascii_digit() || c == '-'
        }

        impl Iterator for CssTokenizer {
            type Item = CssToken;

            fn next(&mut self) -> Option<CssToken> {
                self.skip_comments();
                let c = self.peek(0)?;

                // 先頭の数文字で種類が決まるものを先に見る
                if c.is_whitespace() {
                    while self.peek(0).is_some_and(|c| c.is_whitespace()) {
                        self.pos += 1;
                    }
                    return Some(CssToken::Whitespace);
                }
                if self.starts_number() {
                    return Some(self.consume_numeric());
                }
                if self.starts_name() {
                    let name = self.consume_name();
                    if self.peek(0) == Some('(') {
                        self.pos += 1;
                        return Some(CssToken::Function(name));
                    }
                    return Some(CssToken::Ident(name));
                }

                self.pos += 1;
                Some(match c {
                    '"' | '\'' => self.consume_string(c),
                    '#' if self.peek(0).is_some_and(is_name_char) => CssToken::Hash(self.consume_name()),
                    '@' if self.starts_name() => CssToken::AtKeyword(self.consume_name()),
                    ':' => CssToken::Colon,
                    ';' => CssToken::Semicolon,
                    ',' => CssToken::Comma,
                    '{' => CssToken::OpenCurly,
                    '}' => CssToken::CloseCurly,
                    '(' => CssToken::OpenParen,
                    ')' => CssToken::CloseParen,
                    '[' => CssToken::OpenSquare,
                    ']' => CssToken::CloseSquare,
                    other => CssToken::Delim(other),
                })
            }
        }
    }
    pub mod cssom {
        //! CSSOM。構文解析の出力で、「スタイルシート → ルール → セレクタ + 宣言」の木。
        //!
        //! CSS Syntax Level 3 §5: 「qualified rule は、component value のリストからなる prelude と、
        //! 宣言のリストを持つ」「declaration は名前(文字列)と、component value のリストからなる値を持つ」。
        //! この実装では prelude をセレクタに読み替え、component value はトークンそのものとして持つ。

        use crate::css::token::CssToken;
        use crate::html::dom::Element;

        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct StyleSheet {
            pub rules: Vec<QualifiedRule>,
        }

        #[derive(Debug, Clone, PartialEq)]
        pub struct QualifiedRule {
            /// "h1, h2 { ... }" のようにコンマで並んだセレクタ(セレクタリスト)
            pub selectors: Vec<Selector>,
            pub declarations: Vec<Declaration>,
        }

        /// 単純セレクタ 1 つ。結合子(a b、a > b)や複合セレクタ(p.note)は Unsupported になる
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum Selector {
            /// `*`
            Universal,
            /// `p`
            Type(String),
            /// `.note`
            Class(String),
            /// `#main`
            Id(String),
            /// 読めなかったもの(元の文字列を持つ)。何にも一致しない
            Unsupported(String),
        }

        #[derive(Debug, Clone, PartialEq)]
        pub struct Declaration {
            pub property: String,
            /// 値のトークン列(前後の空白は除いてある)
            pub value: Vec<CssToken>,
        }

        impl Selector {
            /// 要素に一致するか
            pub fn matches(&self, el: &Element) -> bool {
                match self {
                    Selector::Universal => true,
                    Selector::Type(t) => el.tag == *t,
                    Selector::Class(c) => el.classes().contains(&c.as_str()),
                    Selector::Id(i) => el.id() == Some(i.as_str()),
                    Selector::Unsupported(_) => false,
                }
            }

            /// 詳細度 (A, B, C)。Selectors Level 4 §17:
            /// A = ID セレクタの数、B = クラス・属性・擬似クラスの数、C = 型セレクタ・擬似要素の数。
            /// 比較は A → B → C の順(辞書順)。タプルの比較がそのまま仕様の比較になる
            pub fn specificity(&self) -> (u32, u32, u32) {
                match self {
                    Selector::Id(_) => (1, 0, 0),
                    Selector::Class(_) => (0, 1, 0),
                    Selector::Type(_) => (0, 0, 1),
                    Selector::Universal | Selector::Unsupported(_) => (0, 0, 0),
                }
            }

            pub fn to_css(&self) -> String {
                match self {
                    Selector::Universal => "*".to_string(),
                    Selector::Type(t) => t.clone(),
                    Selector::Class(c) => format!(".{c}"),
                    Selector::Id(i) => format!("#{i}"),
                    Selector::Unsupported(s) => format!("{s} /* 未対応 */"),
                }
            }
        }

        impl Declaration {
            pub fn value_text(&self) -> String {
                self.value.iter().map(|t| t.to_css()).collect::<Vec<_>>().join("")
            }
        }

        impl QualifiedRule {
            pub fn to_css(&self) -> String {
                let sel = self.selectors.iter().map(|s| s.to_css()).collect::<Vec<_>>().join(", ");
                let decls = self
                    .declarations
                    .iter()
                    .map(|d| format!("  {}: {};", d.property, d.value_text()))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("{sel} {{\n{decls}\n}}")
            }
        }
    }
    pub mod parser {
        //! CSS の構文解析(CSS Syntax Module Level 3 §5 Parsing)。トークン列 → CSSOM。
        //!
        //! 仕様の芯は「壊れていても止まらない」こと。読めないルールや宣言は、
        //! 「通常の解析に戻れる最小限だけ捨てて」次へ進む。ここでも読めない宣言は飛ばし、
        //! 読めないセレクタは Unsupported として残す(どの要素にも当たらない)。

        use std::iter::Peekable;

        use crate::css::cssom::{Declaration, QualifiedRule, Selector, StyleSheet};
        use crate::css::token::CssToken;
        use crate::css::tokenizer::CssTokenizer;

        pub struct CssParser {
            tokens: Peekable<CssTokenizer>,
        }

        impl CssParser {
            pub fn new(css: &str) -> Self {
                CssParser {
                    tokens: CssTokenizer::new(css).peekable(),
                }
            }

            /// 文字列から StyleSheet を作る
            pub fn parse(css: &str) -> StyleSheet {
                CssParser::new(css).parse_stylesheet()
            }

            /// 仕様 "parse a stylesheet" → "consume a list of rules"
            pub fn parse_stylesheet(mut self) -> StyleSheet {
                let mut rules = Vec::new();
                loop {
                    self.skip_whitespace();
                    match self.tokens.peek() {
                        None => break,
                        Some(CssToken::AtKeyword(_)) => self.skip_at_rule(),
                        Some(_) => {
                            if let Some(rule) = self.consume_qualified_rule() {
                                rules.push(rule);
                            }
                        }
                    }
                }
                StyleSheet { rules }
            }

            fn skip_whitespace(&mut self) {
                while matches!(self.tokens.peek(), Some(CssToken::Whitespace)) {
                    self.tokens.next();
                }
            }

            /// 仕様 "consume a qualified rule": "{" までが prelude、"{ ... }" が宣言のブロック。
            /// "{" の前に入力が終わったら、そのルールは捨てる(parse error)
            fn consume_qualified_rule(&mut self) -> Option<QualifiedRule> {
                let mut prelude = Vec::new();
                loop {
                    match self.tokens.next() {
                        None => return None,
                        Some(CssToken::OpenCurly) => break,
                        Some(t) => prelude.push(t),
                    }
                }
                let selectors = parse_selector_list(&prelude);
                let declarations = self.consume_declarations();
                Some(QualifiedRule {
                    selectors,
                    declarations,
                })
            }

            /// 仕様 "consume a list of declarations"(簡略版)。"}" か入力の終わりまで
            fn consume_declarations(&mut self) -> Vec<Declaration> {
                let mut decls = Vec::new();
                loop {
                    self.skip_whitespace();
                    match self.tokens.next() {
                        None | Some(CssToken::CloseCurly) => break,
                        Some(CssToken::Semicolon) => continue,
                        Some(CssToken::Ident(name)) => {
                            self.skip_whitespace();
                            if !matches!(self.tokens.peek(), Some(CssToken::Colon)) {
                                // "color red;" のように ":" が無い → この宣言は捨てる
                                self.skip_to_end_of_declaration();
                                continue;
                            }
                            self.tokens.next(); // ":"
                            let mut value = Vec::new();
                            loop {
                                match self.tokens.peek() {
                                    None | Some(CssToken::Semicolon) | Some(CssToken::CloseCurly) => break,
                                    Some(_) => value.push(self.tokens.next().unwrap()),
                                }
                            }
                            // 前後の空白を除く
                            while matches!(value.first(), Some(CssToken::Whitespace)) {
                                value.remove(0);
                            }
                            while matches!(value.last(), Some(CssToken::Whitespace)) {
                                value.pop();
                            }
                            if !value.is_empty() {
                                decls.push(Declaration {
                                    property: name,
                                    value,
                                });
                            }
                        }
                        Some(_) => self.skip_to_end_of_declaration(), // 名前が識別子でない → 捨てる
                    }
                }
                decls
            }

            /// 次の ";" まで読み捨てる。"}" は消費しない(ブロックの終わりなので)
            fn skip_to_end_of_declaration(&mut self) {
                loop {
                    match self.tokens.peek() {
                        None | Some(CssToken::CloseCurly) => return,
                        Some(CssToken::Semicolon) => {
                            self.tokens.next();
                            return;
                        }
                        Some(_) => {
                            self.tokens.next();
                        }
                    }
                }
            }

            /// @media などの at-rule を丸ごと読み捨てる。";" で終わるか、{} のブロックが閉じるまで
            fn skip_at_rule(&mut self) {
                let mut depth = 0usize;
                for t in self.tokens.by_ref() {
                    match t {
                        CssToken::Semicolon if depth == 0 => return,
                        CssToken::OpenCurly => depth += 1,
                        CssToken::CloseCurly => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                return;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        /// prelude をセレクタのリストに読み替える。コンマで区切り、各区切りは単純セレクタ 1 つだけを受け付ける
        fn parse_selector_list(prelude: &[CssToken]) -> Vec<Selector> {
            prelude
                .split(|t| *t == CssToken::Comma)
                .map(|group| {
                    let parts: Vec<&CssToken> = group.iter().filter(|t| **t != CssToken::Whitespace).collect();
                    match parts.as_slice() {
                        [CssToken::Ident(tag)] => Selector::Type(tag.to_ascii_lowercase()),
                        [CssToken::Hash(id)] => Selector::Id(id.clone()),
                        [CssToken::Delim('*')] => Selector::Universal,
                        [CssToken::Delim('.'), CssToken::Ident(class)] => Selector::Class(class.clone()),
                        _ => Selector::Unsupported(parts.iter().map(|t| t.to_css()).collect::<Vec<_>>().join("")),
                    }
                })
                .filter(|s| !matches!(s, Selector::Unsupported(text) if text.is_empty()))
                .collect()
        }
    }
}

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
