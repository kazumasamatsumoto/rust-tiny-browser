// ステップ 5: トークンから DOM ツリーを組み立てる(挿入モードと開いている要素のスタック)
//
// このファイル 1 つで動きます:
//   実行:      rustc --edition 2021 step05_dom.rs && ./step05_dom
//   テスト:    rustc --edition 2021 --test step05_dom.rs -o step05_dom_test && ./step05_dom_test
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
    pub mod tokenizer {
        //! HTML の字句解析。文字の列を「状態機械」で読み、トークンの列にする。
        //!
        //! HTML Standard 13.2.5 Tokenization:
        //! 「実装は、次の状態機械を使って HTML をトークン化するかのように振る舞わなければならない。
        //!   状態機械は data state から始まる。ほとんどの状態は 1 文字を消費し、
        //!   新しい状態へ切り替えて今の文字を再消費するか、新しい状態へ切り替えて次の文字を消費するか、
        //!   同じ状態にとどまって次の文字を消費する。」
        //!
        //! 仕様の状態は 84 個(13.2.5.1〜13.2.5.84)。この実装はそのうち 17 個 + 読み捨て用 1 個を持つ。
        //! 状態の名前と遷移の条件は仕様と同じにしてあるので、仕様の該当節と見比べながら読める。
        //!
        //! 持っていないもの: 文字参照(&amp; など)、コメント、DOCTYPE、RCDATA / RAWTEXT の各状態、
        //! script data の escape 系の状態。`<style>` と `<title>` の中身は script と同じ扱いで文字として読む。

        use std::collections::VecDeque;

        use crate::html::token::{Attribute, HtmlToken};

        /// 状態。名前は HTML Standard 13.2.5 の節の名前に合わせてある。
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum State {
            /// 13.2.5.1 タグの外側
            Data,
            /// 13.2.5.6 "<" を読んだ直後
            TagOpen,
            /// 13.2.5.7 "</" を読んだ直後
            EndTagOpen,
            /// 13.2.5.8 タグ名の途中
            TagName,
            /// 13.2.5.32 タグ名のあと、属性名の前
            BeforeAttributeName,
            /// 13.2.5.33 属性名の途中
            AttributeName,
            /// 13.2.5.34 属性名のあと("=" か次の属性か ">" を待つ)
            AfterAttributeName,
            /// 13.2.5.35 "=" のあと、値の前
            BeforeAttributeValue,
            /// 13.2.5.36 "..." の中
            AttributeValueDoubleQuoted,
            /// 13.2.5.37 '...' の中
            AttributeValueSingleQuoted,
            /// 13.2.5.38 引用符なしの値の中
            AttributeValueUnquoted,
            /// 13.2.5.39 閉じ引用符のあと
            AfterAttributeValueQuoted,
            /// 13.2.5.40 "/" のあと(">" が来れば自己終了タグ)
            SelfClosingStartTag,
            /// 13.2.5.4 `<script>` の中身。"<" が来てもタグにしない
            ScriptData,
            /// 13.2.5.15 script の中で "<" を読んだ直後
            ScriptDataLessThanSign,
            /// 13.2.5.16 script の中で "</" を読んだ直後
            ScriptDataEndTagOpen,
            /// 13.2.5.17 script の中の終了タグ名らしきものの途中
            ScriptDataEndTagName,
            /// 13.2.5.41 `<!...>` を ">" まで読み捨てる(DOCTYPE・コメントの代用)
            BogusComment,
        }

        /// 中身を「文字として」読む要素。本物は script data / RAWTEXT / RCDATA の 3 種類の状態を使い分けるが、
        /// 終了タグの見つけ方(「適切な終了タグ」が来るまで文字)は同じなので、この実装は 1 つの状態で扱う。
        const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style", "title", "textarea"];

        pub struct HtmlTokenizer {
            chars: Vec<char>,
            pos: usize,
            state: State,
            /// true なら次の consume() は直前の文字をもう一度返す(仕様の "reconsume")
            reconsume: bool,
            /// 作りかけのタグ
            current: Option<HtmlToken>,
            /// 最後に出した開始タグの名前。script data の中で「適切な終了タグ」かどうかを決めるのに使う
            last_start_tag: String,
            /// script data の中で "</" に続けて読んだ文字。終了タグでなかったら文字として流し直す
            temp: String,
            /// 1 回の遷移で 2 つ以上のトークンを出すときの待ち行列
            pending: VecDeque<HtmlToken>,
            finished: bool,
        }

        impl HtmlTokenizer {
            pub fn new(html: &str) -> Self {
                HtmlTokenizer {
                    chars: html.chars().collect(),
                    pos: 0,
                    state: State::Data,
                    reconsume: false,
                    current: None,
                    last_start_tag: String::new(),
                    temp: String::new(),
                    pending: VecDeque::new(),
                    finished: false,
                }
            }

            /// 今の状態(学習用に外から覗けるようにしてある)
            pub fn state(&self) -> State {
                self.state
            }

            fn consume(&mut self) -> Option<char> {
                if self.reconsume {
                    self.reconsume = false;
                    return self.chars.get(self.pos - 1).copied();
                }
                let c = self.chars.get(self.pos).copied();
                if c.is_some() {
                    self.pos += 1;
                }
                c
            }

            /// 「state に切り替えて、今の文字を再消費する」
            fn reconsume_in(&mut self, state: State) {
                self.reconsume = true;
                self.state = state;
            }

            fn new_start_tag(&mut self) {
                self.current = Some(HtmlToken::StartTag {
                    tag: String::new(),
                    self_closing: false,
                    attributes: Vec::new(),
                });
            }

            fn new_end_tag(&mut self) {
                self.current = Some(HtmlToken::EndTag { tag: String::new() });
            }

            fn append_tag_name(&mut self, c: char) {
                match &mut self.current {
                    Some(HtmlToken::StartTag { tag, .. }) | Some(HtmlToken::EndTag { tag }) => tag.push(c),
                    _ => {}
                }
            }

            fn new_attribute(&mut self) {
                if let Some(HtmlToken::StartTag { attributes, .. }) = &mut self.current {
                    attributes.push(Attribute::new("", ""));
                }
            }

            fn append_attribute_name(&mut self, c: char) {
                if let Some(HtmlToken::StartTag { attributes, .. }) = &mut self.current {
                    if let Some(a) = attributes.last_mut() {
                        a.name.push(c);
                    }
                }
            }

            fn append_attribute_value(&mut self, c: char) {
                if let Some(HtmlToken::StartTag { attributes, .. }) = &mut self.current {
                    if let Some(a) = attributes.last_mut() {
                        a.value.push(c);
                    }
                }
            }

            fn set_self_closing(&mut self) {
                if let Some(HtmlToken::StartTag { self_closing, .. }) = &mut self.current {
                    *self_closing = true;
                }
            }

            /// 作りかけのタグを完成させて返す。次の状態は原則 Data。
            /// ただし開始タグが script などなら、中身を文字として読む ScriptData に入る
            /// (本物では構文解析器がこの切り替えを指示する。ここでは字句解析器が自分で切り替える)。
            fn emit_current_tag(&mut self) -> HtmlToken {
                let token = self.current.take().expect("タグを作っている最中のはず");
                self.state = State::Data;
                if let HtmlToken::StartTag { tag, .. } = &token {
                    self.last_start_tag = tag.clone();
                    if RAW_TEXT_ELEMENTS.contains(&tag.as_str()) {
                        self.state = State::ScriptData;
                    }
                }
                token
            }

            /// 作りかけの終了タグが、最後の開始タグと同じ名前か(仕様の "appropriate end tag token")
            fn is_appropriate_end_tag(&self) -> bool {
                matches!(&self.current, Some(HtmlToken::EndTag { tag }) if *tag == self.last_start_tag)
            }
        }

        fn is_whitespace(c: char) -> bool {
            matches!(c, '\t' | '\n' | '\x0C' | ' ')
        }

        impl Iterator for HtmlTokenizer {
            type Item = HtmlToken;

            fn next(&mut self) -> Option<HtmlToken> {
                if let Some(t) = self.pending.pop_front() {
                    return Some(t);
                }
                if self.finished {
                    return None;
                }
                loop {
                    let c = match self.consume() {
                        Some(c) => c,
                        None => {
                            // どの状態で終わっても EOF トークンを 1 回出して終わる
                            self.finished = true;
                            return Some(HtmlToken::Eof);
                        }
                    };
                    match self.state {
                        State::Data => match c {
                            '<' => self.state = State::TagOpen,
                            _ => return Some(HtmlToken::Char(c)),
                        },

                        State::TagOpen => match c {
                            '!' => self.state = State::BogusComment,
                            '/' => self.state = State::EndTagOpen,
                            c if c.is_ascii_alphabetic() => {
                                self.new_start_tag();
                                self.reconsume_in(State::TagName);
                            }
                            _ => {
                                // "<" の次がタグ名でなければ、"<" はただの文字
                                self.reconsume_in(State::Data);
                                return Some(HtmlToken::Char('<'));
                            }
                        },

                        State::EndTagOpen => match c {
                            c if c.is_ascii_alphabetic() => {
                                self.new_end_tag();
                                self.reconsume_in(State::TagName);
                            }
                            '>' => self.state = State::Data, // "</>" は読み捨て
                            _ => self.reconsume_in(State::BogusComment),
                        },

                        State::TagName => match c {
                            c if is_whitespace(c) => self.state = State::BeforeAttributeName,
                            '/' => self.state = State::SelfClosingStartTag,
                            '>' => return Some(self.emit_current_tag()),
                            _ => self.append_tag_name(c.to_ascii_lowercase()),
                        },

                        State::BeforeAttributeName => match c {
                            c if is_whitespace(c) => {}
                            '/' | '>' => self.reconsume_in(State::AfterAttributeName),
                            _ => {
                                self.new_attribute();
                                self.reconsume_in(State::AttributeName);
                            }
                        },

                        State::AttributeName => match c {
                            c if is_whitespace(c) || c == '/' || c == '>' => self.reconsume_in(State::AfterAttributeName),
                            '=' => self.state = State::BeforeAttributeValue,
                            _ => self.append_attribute_name(c.to_ascii_lowercase()),
                        },

                        State::AfterAttributeName => match c {
                            c if is_whitespace(c) => {}
                            '/' => self.state = State::SelfClosingStartTag,
                            '=' => self.state = State::BeforeAttributeValue,
                            '>' => return Some(self.emit_current_tag()),
                            _ => {
                                // 値のない属性の次の属性(例: <input disabled required>)
                                self.new_attribute();
                                self.reconsume_in(State::AttributeName);
                            }
                        },

                        State::BeforeAttributeValue => match c {
                            c if is_whitespace(c) => {}
                            '"' => self.state = State::AttributeValueDoubleQuoted,
                            '\'' => self.state = State::AttributeValueSingleQuoted,
                            '>' => return Some(self.emit_current_tag()), // 値が無い(parse error だがタグは出す)
                            _ => self.reconsume_in(State::AttributeValueUnquoted),
                        },

                        State::AttributeValueDoubleQuoted => match c {
                            '"' => self.state = State::AfterAttributeValueQuoted,
                            _ => self.append_attribute_value(c),
                        },

                        State::AttributeValueSingleQuoted => match c {
                            '\'' => self.state = State::AfterAttributeValueQuoted,
                            _ => self.append_attribute_value(c),
                        },

                        State::AttributeValueUnquoted => match c {
                            c if is_whitespace(c) => self.state = State::BeforeAttributeName,
                            '>' => return Some(self.emit_current_tag()),
                            _ => self.append_attribute_value(c),
                        },

                        State::AfterAttributeValueQuoted => match c {
                            c if is_whitespace(c) => self.state = State::BeforeAttributeName,
                            '/' => self.state = State::SelfClosingStartTag,
                            '>' => return Some(self.emit_current_tag()),
                            _ => self.reconsume_in(State::BeforeAttributeName), // 引用符の直後に空白が無い(parse error)
                        },

                        State::SelfClosingStartTag => match c {
                            '>' => {
                                self.set_self_closing();
                                return Some(self.emit_current_tag());
                            }
                            _ => self.reconsume_in(State::BeforeAttributeName),
                        },

                        State::ScriptData => match c {
                            '<' => self.state = State::ScriptDataLessThanSign,
                            _ => return Some(HtmlToken::Char(c)),
                        },

                        State::ScriptDataLessThanSign => match c {
                            '/' => {
                                self.temp.clear();
                                self.state = State::ScriptDataEndTagOpen;
                            }
                            _ => {
                                self.reconsume_in(State::ScriptData);
                                return Some(HtmlToken::Char('<'));
                            }
                        },

                        State::ScriptDataEndTagOpen => match c {
                            c if c.is_ascii_alphabetic() => {
                                self.new_end_tag();
                                self.reconsume_in(State::ScriptDataEndTagName);
                            }
                            _ => {
                                self.reconsume_in(State::ScriptData);
                                self.pending.push_back(HtmlToken::Char('/'));
                                return Some(HtmlToken::Char('<'));
                            }
                        },

                        State::ScriptDataEndTagName => {
                            let appropriate = self.is_appropriate_end_tag();
                            match c {
                                c if is_whitespace(c) && appropriate => self.state = State::BeforeAttributeName,
                                '/' if appropriate => self.state = State::SelfClosingStartTag,
                                '>' if appropriate => return Some(self.emit_current_tag()),
                                c if c.is_ascii_alphabetic() => {
                                    self.append_tag_name(c.to_ascii_lowercase());
                                    self.temp.push(c);
                                }
                                _ => {
                                    // 終了タグではなかった(例: script の中の "</scr" + "ipt>" 以外の文字列)。
                                    // "</" と読んだ文字をそのまま文字として流し、script data に戻る
                                    self.current = None;
                                    self.pending.push_back(HtmlToken::Char('/'));
                                    for t in self.temp.chars() {
                                        self.pending.push_back(HtmlToken::Char(t));
                                    }
                                    self.reconsume_in(State::ScriptData);
                                    return Some(HtmlToken::Char('<'));
                                }
                            }
                        }

                        State::BogusComment => match c {
                            '>' => self.state = State::Data,
                            _ => {}
                        },
                    }
                }
            }
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
    pub mod parser {
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
    }
}

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
