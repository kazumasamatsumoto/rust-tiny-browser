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
