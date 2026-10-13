// ステップ 4: HTML を状態機械でトークンに切る
//
// このファイル 1 つで動きます:
//   実行:      rustc --edition 2021 step04_html_tokenizer.rs && ./step04_html_tokenizer
//   テスト:    rustc --edition 2021 --test step04_html_tokenizer.rs -o step04_html_tokenizer_test && ./step04_html_tokenizer_test
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

        #[cfg(test)]
        mod tests {
            use super::*;

            fn tokens(html: &str) -> Vec<HtmlToken> {
                HtmlTokenizer::new(html).collect()
            }

            fn start(tag: &str, attrs: &[(&str, &str)]) -> HtmlToken {
                HtmlToken::StartTag {
                    tag: tag.to_string(),
                    self_closing: false,
                    attributes: attrs.iter().map(|(n, v)| Attribute::new(n, v)).collect(),
                }
            }

            fn end(tag: &str) -> HtmlToken {
                HtmlToken::EndTag { tag: tag.to_string() }
            }

            fn chars(s: &str) -> Vec<HtmlToken> {
                s.chars().map(HtmlToken::Char).collect()
            }

            #[test]
            fn empty_input_gives_only_eof() {
                assert_eq!(tokens(""), vec![HtmlToken::Eof]);
            }

            #[test]
            fn start_and_end_tag() {
                let mut expected = vec![start("p", &[])];
                expected.extend(chars("hi"));
                expected.push(end("p"));
                expected.push(HtmlToken::Eof);
                assert_eq!(tokens("<p>hi</p>"), expected);
            }

            #[test]
            fn tag_names_are_lowercased() {
                assert_eq!(tokens("<DIV></Div>"), vec![start("div", &[]), end("div"), HtmlToken::Eof]);
            }

            #[test]
            fn attributes_in_three_quote_styles() {
                let t = tokens(r#"<a href="/x" title='t' target=_blank>"#);
                assert_eq!(
                    t[0],
                    start("a", &[("href", "/x"), ("title", "t"), ("target", "_blank")])
                );
            }

            #[test]
            fn attribute_without_value_and_self_closing() {
                let t = tokens("<input disabled required/>");
                assert_eq!(
                    t[0],
                    HtmlToken::StartTag {
                        tag: "input".into(),
                        self_closing: true,
                        attributes: vec![Attribute::new("disabled", ""), Attribute::new("required", "")],
                    }
                );
            }

            #[test]
            fn lone_less_than_is_a_character() {
                let t = tokens("a < b");
                assert_eq!(t, {
                    let mut v = chars("a < b");
                    v.push(HtmlToken::Eof);
                    v
                });
            }

            #[test]
            fn script_content_is_not_parsed_as_tags() {
                let t = tokens("<script>if (a < b) { x = '</div>'; }</script>");
                let body: String = t[1..t.len() - 2]
                    .iter()
                    .map(|t| match t {
                        HtmlToken::Char(c) => *c,
                        other => panic!("script の中にタグが出た: {other:?}"),
                    })
                    .collect();
                assert_eq!(body, "if (a < b) { x = '</div>'; }");
                assert_eq!(t[t.len() - 2], end("script"));
            }

            #[test]
            fn doctype_and_comment_are_skipped() {
                assert_eq!(
                    tokens("<!DOCTYPE html><!-- c --><p>"),
                    vec![start("p", &[]), HtmlToken::Eof]
                );
            }
        }
    }
}

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
