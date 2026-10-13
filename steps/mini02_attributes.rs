// 最小の HTML 字句解析器(その 2): 属性を読む 8 つの状態を足す
//
// 足した状態: BeforeAttributeName / AttributeName / AfterAttributeName / BeforeAttributeValue /
//            AttributeValue(DoubleQuoted / SingleQuoted / Unquoted) / AfterAttributeValueQuoted
// 名前は HTML Standard 13.2.5.32〜13.2.5.39 と同じ。
//
//   実行: rustc --edition 2021 mini02_attributes.rs && ./mini02_attributes

#[derive(Debug, Clone, PartialEq)]
struct Attribute {
    name: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq)]
enum HtmlToken {
    StartTag { tag: String, attributes: Vec<Attribute> },
    EndTag { tag: String },
    Char(char),
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Data,
    TagOpen,
    EndTagOpen,
    TagName,
    BeforeAttributeName,        // タグ名のあとの空白を読んだ
    AttributeName,              // 属性名の途中
    AfterAttributeName,         // 属性名のあと。"=" か次の属性か ">" を待つ
    BeforeAttributeValue,       // "=" のあと
    AttributeValueDoubleQuoted, // "..." の中
    AttributeValueSingleQuoted, // '...' の中
    AttributeValueUnquoted,     // 引用符なしの値
    AfterAttributeValueQuoted,  // 閉じ引用符のあと
}

struct Tokenizer {
    chars: Vec<char>,
    pos: usize,
    state: State,
    reconsume: bool,
    current: Option<HtmlToken>,
    finished: bool,
}

fn is_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0C' | ' ')
}

impl Tokenizer {
    fn new(html: &str) -> Self {
        Tokenizer { chars: html.chars().collect(), pos: 0, state: State::Data, reconsume: false, current: None, finished: false }
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

    fn reconsume_in(&mut self, state: State) {
        self.reconsume = true;
        self.state = state;
    }

    fn append_tag_name(&mut self, c: char) {
        match &mut self.current {
            Some(HtmlToken::StartTag { tag, .. }) | Some(HtmlToken::EndTag { tag }) => tag.push(c),
            _ => {}
        }
    }

    /// 新しい属性を(名前も値も空で)追加する。以後の文字はこの属性に溜まる
    fn new_attribute(&mut self) {
        if let Some(HtmlToken::StartTag { attributes, .. }) = &mut self.current {
            attributes.push(Attribute { name: String::new(), value: String::new() });
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

    fn emit_tag(&mut self) -> HtmlToken {
        self.state = State::Data;
        self.current.take().expect("タグを作っている最中のはず")
    }
}

impl Iterator for Tokenizer {
    type Item = HtmlToken;

    fn next(&mut self) -> Option<HtmlToken> {
        if self.finished {
            return None;
        }
        loop {
            let c = match self.consume() {
                Some(c) => c,
                None => {
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
                    '/' => self.state = State::EndTagOpen,
                    c if c.is_ascii_alphabetic() => {
                        self.current = Some(HtmlToken::StartTag { tag: String::new(), attributes: Vec::new() });
                        self.reconsume_in(State::TagName);
                    }
                    _ => {
                        self.reconsume_in(State::Data);
                        return Some(HtmlToken::Char('<'));
                    }
                },
                State::EndTagOpen => match c {
                    c if c.is_ascii_alphabetic() => {
                        self.current = Some(HtmlToken::EndTag { tag: String::new() });
                        self.reconsume_in(State::TagName);
                    }
                    _ => self.reconsume_in(State::Data),
                },
                State::TagName => match c {
                    c if is_whitespace(c) => self.state = State::BeforeAttributeName, // ← ここが新しい入口
                    '>' => return Some(self.emit_tag()),
                    _ => self.append_tag_name(c.to_ascii_lowercase()),
                },

                // 13.2.5.32 空白は読み飛ばす。">" ならタグ完成。それ以外が来たら新しい属性の始まり
                State::BeforeAttributeName => match c {
                    c if is_whitespace(c) => {}
                    '>' => self.reconsume_in(State::AfterAttributeName),
                    _ => {
                        self.new_attribute();
                        self.reconsume_in(State::AttributeName);
                    }
                },
                // 13.2.5.33 属性名。空白か ">" で名前の終わり、"=" で値へ。名前は小文字に
                State::AttributeName => match c {
                    c if is_whitespace(c) || c == '>' => self.reconsume_in(State::AfterAttributeName),
                    '=' => self.state = State::BeforeAttributeValue,
                    _ => self.append_attribute_name(c.to_ascii_lowercase()),
                },
                // 13.2.5.34 名前のあと。"=" なら値、">" なら完成、それ以外は(値のない属性の)次の属性
                State::AfterAttributeName => match c {
                    c if is_whitespace(c) => {}
                    '=' => self.state = State::BeforeAttributeValue,
                    '>' => return Some(self.emit_tag()),
                    _ => {
                        self.new_attribute();
                        self.reconsume_in(State::AttributeName);
                    }
                },
                // 13.2.5.35 "=" のあと。引用符の種類で 3 つの状態に分かれる
                State::BeforeAttributeValue => match c {
                    c if is_whitespace(c) => {}
                    '"' => self.state = State::AttributeValueDoubleQuoted,
                    '\'' => self.state = State::AttributeValueSingleQuoted,
                    '>' => return Some(self.emit_tag()),
                    _ => self.reconsume_in(State::AttributeValueUnquoted),
                },
                // 13.2.5.36 / 37: 閉じ引用符が来るまで全部が値(空白も ">" も)
                State::AttributeValueDoubleQuoted => match c {
                    '"' => self.state = State::AfterAttributeValueQuoted,
                    _ => self.append_attribute_value(c),
                },
                State::AttributeValueSingleQuoted => match c {
                    '\'' => self.state = State::AfterAttributeValueQuoted,
                    _ => self.append_attribute_value(c),
                },
                // 13.2.5.38 引用符なし: 空白か ">" で値の終わり
                State::AttributeValueUnquoted => match c {
                    c if is_whitespace(c) => self.state = State::BeforeAttributeName,
                    '>' => return Some(self.emit_tag()),
                    _ => self.append_attribute_value(c),
                },
                // 13.2.5.39 閉じ引用符のあと: 空白なら次の属性へ、">" なら完成
                State::AfterAttributeValueQuoted => match c {
                    c if is_whitespace(c) => self.state = State::BeforeAttributeName,
                    '>' => return Some(self.emit_tag()),
                    _ => self.reconsume_in(State::BeforeAttributeName),
                },
            }
        }
    }
}

fn main() {
    let inputs = [
        r#"<a href="/x" title='t' target=_blank>"#,
        r#"<input disabled required>"#,
        r#"<p class="a b" ID=Main>"#,
        r#"<img alt="a > b">"#,
    ];
    for html in inputs {
        println!("── {html}");
        for token in Tokenizer::new(html) {
            println!("   {token:?}");
        }
    }
}
