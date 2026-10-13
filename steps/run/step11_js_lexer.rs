// ステップ 11: JavaScript をトークンに切る
//
// このファイル 1 つで動きます:
//   実行:      rustc --edition 2021 step11_js_lexer.rs && ./step11_js_lexer
//   テスト:    rustc --edition 2021 --test step11_js_lexer.rs -o step11_js_lexer_test && ./step11_js_lexer_test
// cargo のプロジェクトでは、各 `mod` の中身がそのまま src/ 以下のファイルになります。
#![allow(dead_code)] // 部品を全部 1 ファイルに入れているので、このステップで使わない関数もある

mod js {
    pub mod lexer {
        //! JavaScript の字句解析(ECMAScript §12 Lexical Grammar の、ごく一部)。
        //!
        //! 扱うトークン: 区切り記号(+ - = ( ) { } ; . ,)、数値、文字列、識別子、予約語(var let const function return)。
        //! 2 文字の演算子(=== += など)、正規表現リテラル、テンプレートリテラル、コメントの /* */ は扱わない。

        #[derive(Debug, Clone, PartialEq)]
        pub enum Token {
            /// + - = ( ) { } ; . ,
            Punctuator(char),
            Number(f64),
            Str(String),
            Identifier(String),
            /// var let const function return
            Keyword(String),
        }

        pub const KEYWORDS: &[&str] = &["var", "let", "const", "function", "return"];

        pub struct JsLexer {
            chars: Vec<char>,
            pos: usize,
        }

        impl JsLexer {
            pub fn new(src: &str) -> Self {
                JsLexer {
                    chars: src.chars().collect(),
                    pos: 0,
                }
            }

            fn peek(&self) -> Option<char> {
                self.chars.get(self.pos).copied()
            }

            fn skip_whitespace_and_comments(&mut self) {
                loop {
                    match self.peek() {
                        Some(c) if c.is_whitespace() => self.pos += 1,
                        Some('/') if self.chars.get(self.pos + 1) == Some(&'/') => {
                            while let Some(c) = self.peek() {
                                self.pos += 1;
                                if c == '\n' {
                                    break;
                                }
                            }
                        }
                        _ => return,
                    }
                }
            }
        }

        fn is_ident_start(c: char) -> bool {
            c.is_ascii_alphabetic() || c == '_' || c == '$'
        }

        fn is_ident_part(c: char) -> bool {
            is_ident_start(c) || c.is_ascii_digit()
        }

        impl Iterator for JsLexer {
            type Item = Token;

            fn next(&mut self) -> Option<Token> {
                self.skip_whitespace_and_comments();
                let c = self.peek()?;

                if c.is_ascii_digit() {
                    let mut s = String::new();
                    while let Some(d) = self.peek() {
                        if d.is_ascii_digit() || d == '.' {
                            s.push(d);
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                    return Some(Token::Number(s.parse().unwrap_or(0.0)));
                }

                if is_ident_start(c) {
                    let mut s = String::new();
                    while let Some(d) = self.peek() {
                        if is_ident_part(d) {
                            s.push(d);
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                    return Some(if KEYWORDS.contains(&s.as_str()) {
                        Token::Keyword(s)
                    } else {
                        Token::Identifier(s)
                    });
                }

                self.pos += 1;
                match c {
                    '"' | '\'' => {
                        let mut s = String::new();
                        while let Some(d) = self.peek() {
                            self.pos += 1;
                            if d == c {
                                break;
                            }
                            if d == '\\' {
                                if let Some(e) = self.peek() {
                                    self.pos += 1;
                                    s.push(match e {
                                        'n' => '\n',
                                        't' => '\t',
                                        other => other,
                                    });
                                }
                                continue;
                            }
                            s.push(d);
                        }
                        Some(Token::Str(s))
                    }
                    '+' | '-' | '=' | '(' | ')' | '{' | '}' | ';' | '.' | ',' => Some(Token::Punctuator(c)),
                    other => Some(Token::Punctuator(other)), // 知らない記号もそのまま流す(構文解析で弾く)
                }
            }
        }
    }
}

use js::lexer::JsLexer;

fn show(src: &str) {
    println!("── {src:?}");
    for t in JsLexer::new(src) {
        println!("   {t:?}");
    }
    println!();
}

fn main() {
    show("1 + 2");
    show("var x = 42;");
    show("function add(a, b) { return a + b; }");
    show("document.getElementById(\"title\").textContent = 'hi'");
    show("x = x - 1 // コメントは消える");
}
