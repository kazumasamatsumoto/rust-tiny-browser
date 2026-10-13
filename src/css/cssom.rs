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
