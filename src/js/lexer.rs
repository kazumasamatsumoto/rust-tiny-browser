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

#[cfg(test)]
mod tests {
    use super::*;
    use super::Token::*;

    fn toks(src: &str) -> Vec<Token> {
        JsLexer::new(src).collect()
    }

    #[test]
    fn empty() {
        assert!(toks("").is_empty());
    }

    #[test]
    fn numbers_and_plus() {
        assert_eq!(toks("1 + 2.5"), vec![Number(1.0), Punctuator('+'), Number(2.5)]);
    }

    #[test]
    fn variable_declaration() {
        assert_eq!(
            toks("var x = 42;"),
            vec![Keyword("var".into()), Identifier("x".into()), Punctuator('='), Number(42.0), Punctuator(';')]
        );
    }

    #[test]
    fn strings_and_member_access() {
        assert_eq!(
            toks("document.getElementById(\"title\")"),
            vec![
                Identifier("document".into()),
                Punctuator('.'),
                Identifier("getElementById".into()),
                Punctuator('('),
                Str("title".into()),
                Punctuator(')'),
            ]
        );
        assert_eq!(toks("'a\\nb'"), vec![Str("a\nb".into())]);
    }

    #[test]
    fn line_comment_is_skipped() {
        assert_eq!(toks("1 // one\n+ 2"), vec![Number(1.0), Punctuator('+'), Number(2.0)]);
    }
}
