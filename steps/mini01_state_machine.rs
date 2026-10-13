// 最小の HTML 字句解析器(その 1): 状態機械でタグと文字を切り分ける
//
// HTML Standard 13.2.5 の 84 個の状態のうち、Data / TagOpen / EndTagOpen / TagName の 4 つだけ。
// 属性・自己終了・script の中身は次のステップで足す。
//
//   実行: rustc --edition 2021 mini01_state_machine.rs && ./mini01_state_machine

#[derive(Debug, Clone, PartialEq)]
enum HtmlToken {
    StartTag { tag: String },
    EndTag { tag: String },
    Char(char),
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Data,       // タグの外側
    TagOpen,    // "<" を読んだ直後
    EndTagOpen, // "</" を読んだ直後
    TagName,    // タグ名の途中
}

struct Tokenizer {
    chars: Vec<char>,
    pos: usize,
    state: State,
    reconsume: bool,           // true なら次に読む文字は「直前の文字をもう一度」
    current: Option<HtmlToken>, // 作りかけのタグ
    finished: bool,
}

impl Tokenizer {
    fn new(html: &str) -> Self {
        Tokenizer {
            chars: html.chars().collect(),
            pos: 0,
            state: State::Data,
            reconsume: false,
            current: None,
            finished: false,
        }
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

    /// 「state に切り替えて、今の文字をもう一度読む」(仕様の reconsume)
    fn reconsume_in(&mut self, state: State) {
        self.reconsume = true;
        self.state = state;
    }

    fn append_tag_name(&mut self, c: char) {
        match &mut self.current {
            Some(HtmlToken::StartTag { tag }) | Some(HtmlToken::EndTag { tag }) => tag.push(c),
            _ => {}
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
                // 13.2.5.1 Data state: "<" ならタグの始まり、それ以外は文字をそのまま出す
                State::Data => match c {
                    '<' => self.state = State::TagOpen,
                    _ => return Some(HtmlToken::Char(c)),
                },
                // 13.2.5.6 Tag open state: "/" なら終了タグ、英字ならタグ名、それ以外なら "<" はただの文字
                State::TagOpen => match c {
                    '/' => self.state = State::EndTagOpen,
                    c if c.is_ascii_alphabetic() => {
                        self.current = Some(HtmlToken::StartTag { tag: String::new() });
                        self.reconsume_in(State::TagName);
                    }
                    _ => {
                        self.reconsume_in(State::Data);
                        return Some(HtmlToken::Char('<'));
                    }
                },
                // 13.2.5.7 End tag open state
                State::EndTagOpen => match c {
                    c if c.is_ascii_alphabetic() => {
                        self.current = Some(HtmlToken::EndTag { tag: String::new() });
                        self.reconsume_in(State::TagName);
                    }
                    _ => self.reconsume_in(State::Data),
                },
                // 13.2.5.8 Tag name state: ">" で完成。英字は小文字にして溜める
                State::TagName => match c {
                    '>' => return Some(self.emit_tag()),
                    _ => self.append_tag_name(c.to_ascii_lowercase()),
                },
            }
        }
    }
}

fn main() {
    for html in ["<p>hi</p>", "<DIV>a<BR>b</DIV>", "1 < 2", "<p>閉じ忘れ"] {
        println!("── {html:?}");
        for token in Tokenizer::new(html) {
            println!("   {token:?}");
        }
    }
}
