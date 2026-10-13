// ステップ 6: CSS をトークンに切る
//
// このファイル 1 つで動きます:
//   実行:      rustc --edition 2021 step06_css_tokenizer.rs && ./step06_css_tokenizer
//   テスト:    rustc --edition 2021 --test step06_css_tokenizer.rs -o step06_css_tokenizer_test && ./step06_css_tokenizer_test
// cargo のプロジェクトでは、各 `mod` の中身がそのまま src/ 以下のファイルになります。
#![allow(dead_code)] // 部品を全部 1 ファイルに入れているので、このステップで使わない関数もある

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
}

use css::tokenizer::CssTokenizer;

fn show(css: &str) {
    println!("── 入力: {css:?}");
    for t in CssTokenizer::new(css) {
        println!("   {t:?}");
    }
    println!();
}

fn main() {
    show("p { color: red; }");
    show("#main .note { font-size: 16px; width: 50%; margin: -2px }");
    show("@media (max-width: 600px) { a { color: #ff0000 } }");
    show("font-family: \"Noto Sans\", sans-serif; /* コメントは消える */");
    show("background: rgb(1, 2, 3)");
}
