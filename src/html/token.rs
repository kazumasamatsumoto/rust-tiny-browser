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
