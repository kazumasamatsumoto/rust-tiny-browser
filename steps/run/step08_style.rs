// ステップ 8: 要素ごとの計算値を決める(カスケード・詳細度・継承)
//
// このファイル 1 つで動きます:
//   実行:      rustc --edition 2021 step08_style.rs && ./step08_style
//   テスト:    rustc --edition 2021 --test step08_style.rs -o step08_style_test && ./step08_style_test
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

mod renderer {
    pub mod style {
        //! スタイルの計算。要素 1 つについて「どの宣言が勝つか(カスケード)」と
        //! 「宣言が無いときに何になるか(継承か初期値か)」を決め、計算値(computed value)を出す。
        //!
        //! CSS Cascading and Inheritance Level 4:
        //! - 「指定値は、スタイルシートの作者がその要素に意図した値」
        //! - 「計算値は、指定値をプロパティ定義表の Computed Value の行に従って解決した結果」
        //! - カスケードの並べ替えは、出所と重要度 → 文脈 → 詳細度 → 出現順。「最後の宣言が勝つ」
        //! - 「継承値は、親要素におけるそのプロパティの計算値」
        //!
        //! この実装が扱うプロパティ: display, color, background-color, font-size, text-decoration, width, height。
        //! 継承するのは color / font-size / text-decoration(本物の text-decoration は継承しないが、
        //! 子孫に線が引かれて見えるので、ここでは単純化して継承扱いにしている)。

        use crate::css::cssom::{Declaration, StyleSheet};
        use crate::css::token::CssToken;
        use crate::html::dom::Element;

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct Color {
            pub r: u8,
            pub g: u8,
            pub b: u8,
        }

        impl Color {
            pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
            pub const WHITE: Color = Color { r: 255, g: 255, b: 255 };
            pub const BLUE: Color = Color { r: 0, g: 0, b: 238 }; // UA の既定のリンク色に近い値

            /// 名前つきの色のうち、よく使うものだけ(CSS Color Level 4 の named colors の一部)
            pub fn from_name(name: &str) -> Option<Color> {
                let (r, g, b) = match name.to_ascii_lowercase().as_str() {
                    "black" => (0, 0, 0),
                    "white" => (255, 255, 255),
                    "red" => (255, 0, 0),
                    "green" => (0, 128, 0),
                    "blue" => (0, 0, 255),
                    "yellow" => (255, 255, 0),
                    "orange" => (255, 165, 0),
                    "gray" | "grey" => (128, 128, 128),
                    "lightgray" | "lightgrey" => (211, 211, 211),
                    "silver" => (192, 192, 192),
                    "navy" => (0, 0, 128),
                    "teal" => (0, 128, 128),
                    "purple" => (128, 0, 128),
                    "pink" => (255, 192, 203),
                    "lightyellow" => (255, 255, 224),
                    "lightblue" => (173, 216, 230),
                    _ => return None,
                };
                Some(Color { r, g, b })
            }

            /// #rgb / #rrggbb
            pub fn from_hex(hex: &str) -> Option<Color> {
                let v = |s: &str| u8::from_str_radix(s, 16).ok();
                match hex.len() {
                    3 => {
                        let d = |i: usize| v(&hex[i..i + 1]).map(|x| x * 17);
                        Some(Color { r: d(0)?, g: d(1)?, b: d(2)? })
                    }
                    6 => Some(Color {
                        r: v(&hex[0..2])?,
                        g: v(&hex[2..4])?,
                        b: v(&hex[4..6])?,
                    }),
                    _ => None,
                }
            }

            /// 宣言の値(トークン列)から色を読む。名前か #hex だけ(rgb() は未対応)
            pub fn from_tokens(tokens: &[CssToken]) -> Option<Color> {
                match tokens.first()? {
                    CssToken::Ident(name) => Color::from_name(name),
                    CssToken::Hash(hex) => Color::from_hex(hex),
                    _ => None,
                }
            }

            pub fn hex(&self) -> String {
                format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Display {
            Block,
            Inline,
            None,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum TextDecoration {
            None,
            Underline,
        }

        /// 要素 1 つの計算値。
        #[derive(Debug, Clone, PartialEq)]
        pub struct ComputedStyle {
            pub display: Display,
            pub color: Color,
            /// None = transparent(初期値)
            pub background_color: Option<Color>,
            /// px
            pub font_size: f32,
            pub text_decoration: TextDecoration,
            /// None = auto
            pub width: Option<f32>,
            pub height: Option<f32>,
        }

        /// 初期値(どの要素でもない「素の」値)
        pub const INITIAL_FONT_SIZE: f32 = 16.0;

        impl Default for ComputedStyle {
            fn default() -> Self {
                ComputedStyle {
                    display: Display::Inline, // display の初期値は inline
                    color: Color::BLACK,      // 本物の初期値は CanvasText。黒として扱う
                    background_color: None,
                    font_size: INITIAL_FONT_SIZE,
                    text_decoration: TextDecoration::None,
                    width: None,
                    height: None,
                }
            }
        }

        /// 要素ごとの計算値を決める。
        ///
        /// 1. 初期値から始める
        /// 2. 継承プロパティは親の計算値で上書きする(宣言が無ければこれが残る)
        /// 3. UA(ブラウザ既定)の宣言を当てる。h1 を大きく、a を青い下線に、head を display:none に、など
        /// 4. 作者の宣言を、一致するルールから集め、詳細度 → 出現順で並べて順に当てる(後の方が勝つ)
        pub fn compute_style(el: &Element, sheet: &StyleSheet, parent: Option<&ComputedStyle>) -> ComputedStyle {
            let mut style = ComputedStyle::default();

            if let Some(p) = parent {
                style.color = p.color;
                style.font_size = p.font_size;
                style.text_decoration = p.text_decoration;
            }

            apply_user_agent_defaults(&mut style, &el.tag);

            // (詳細度, 出現順, 宣言) を集めて安定ソートする
            let mut matched: Vec<((u32, u32, u32), usize, &Declaration)> = Vec::new();
            for (order, rule) in sheet.rules.iter().enumerate() {
                // セレクタリストの中で最も詳細度が高いものが、そのルールの詳細度
                let best = rule
                    .selectors
                    .iter()
                    .filter(|s| s.matches(el))
                    .map(|s| s.specificity())
                    .max();
                if let Some(spec) = best {
                    for d in &rule.declarations {
                        matched.push((spec, order, d));
                    }
                }
            }
            matched.sort_by_key(|(spec, order, _)| (*spec, *order));
            for (_, _, d) in matched {
                apply_declaration(&mut style, d);
            }
            style
        }

        /// 一致した作者の宣言を、勝った順に説明つきで返す(学習用: なぜその値になったかを見せる)
        pub fn explain_cascade(el: &Element, sheet: &StyleSheet) -> Vec<String> {
            let mut matched: Vec<((u32, u32, u32), usize, String, &Declaration)> = Vec::new();
            for (order, rule) in sheet.rules.iter().enumerate() {
                for s in rule.selectors.iter().filter(|s| s.matches(el)) {
                    for d in &rule.declarations {
                        matched.push((s.specificity(), order, s.to_css(), d));
                    }
                }
            }
            matched.sort_by_key(|(spec, order, _, _)| (*spec, *order));
            matched
                .iter()
                .map(|(spec, order, sel, d)| {
                    format!(
                        "{sel:<10} 詳細度 ({},{},{}) 出現順 {}  →  {}: {}",
                        spec.0,
                        spec.1,
                        spec.2,
                        order + 1,
                        d.property,
                        d.value_text()
                    )
                })
                .collect()
        }

        /// ブラウザ既定のスタイル(UA stylesheet)の最小版。HTML Standard 15 Rendering の一部に相当
        fn apply_user_agent_defaults(style: &mut ComputedStyle, tag: &str) {
            style.display = match tag {
                "head" | "style" | "script" | "title" | "meta" | "link" => Display::None,
                "html" | "body" | "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "li" | "section"
                | "article" | "header" | "footer" | "nav" | "main" | "pre" | "blockquote" | "form" | "table" | "hr" => {
                    Display::Block
                }
                _ => Display::Inline,
            };
            match tag {
                "h1" => style.font_size = 32.0, // 2em
                "h2" => style.font_size = 24.0, // 1.5em
                "h3" => style.font_size = 18.72, // 1.17em
                "a" => {
                    style.color = Color::BLUE;
                    style.text_decoration = TextDecoration::Underline;
                }
                _ => {}
            }
        }

        /// 宣言 1 つを計算値に反映する。読めない値は無視する(CSS は読めない宣言を捨てる)
        fn apply_declaration(style: &mut ComputedStyle, d: &Declaration) {
            let first = d.value.first();
            match d.property.as_str() {
                "display" => {
                    if let Some(CssToken::Ident(v)) = first {
                        match v.as_str() {
                            "block" => style.display = Display::Block,
                            "inline" => style.display = Display::Inline,
                            "none" => style.display = Display::None,
                            _ => {}
                        }
                    }
                }
                "color" => {
                    if let Some(c) = Color::from_tokens(&d.value) {
                        style.color = c;
                    }
                }
                "background-color" | "background" => match first {
                    Some(CssToken::Ident(v)) if v == "transparent" => style.background_color = None,
                    _ => {
                        if let Some(c) = Color::from_tokens(&d.value) {
                            style.background_color = Some(c);
                        }
                    }
                },
                "font-size" => match first {
                    Some(CssToken::Dimension(n, unit)) if unit == "px" => style.font_size = *n as f32,
                    Some(CssToken::Ident(v)) => {
                        // 絶対サイズのキーワード(CSS Fonts の absolute-size)。medium を 16px とした比
                        style.font_size = match v.as_str() {
                            "x-small" => 10.0,
                            "small" => 13.0,
                            "medium" => 16.0,
                            "large" => 18.0,
                            "x-large" => 24.0,
                            "xx-large" => 32.0,
                            _ => style.font_size,
                        };
                    }
                    _ => {}
                },
                "text-decoration" | "text-decoration-line" => {
                    if let Some(CssToken::Ident(v)) = first {
                        match v.as_str() {
                            "underline" => style.text_decoration = TextDecoration::Underline,
                            "none" => style.text_decoration = TextDecoration::None,
                            _ => {}
                        }
                    }
                }
                "width" => style.width = px_or_auto(first),
                "height" => style.height = px_or_auto(first),
                _ => {} // 未対応のプロパティは無視
            }
        }

        fn px_or_auto(token: Option<&CssToken>) -> Option<f32> {
            match token {
                Some(CssToken::Dimension(n, unit)) if unit == "px" => Some(*n as f32),
                _ => None,
            }
        }
    }
}

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
