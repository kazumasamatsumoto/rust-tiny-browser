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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::parser::CssParser;
    use crate::html::token::Attribute;

    fn el(tag: &str, attrs: &[(&str, &str)]) -> Element {
        Element {
            tag: tag.into(),
            attributes: attrs.iter().map(|(n, v)| Attribute::new(n, v)).collect(),
        }
    }

    #[test]
    fn user_agent_defaults() {
        let sheet = StyleSheet::default();
        assert_eq!(compute_style(&el("p", &[]), &sheet, None).display, Display::Block);
        assert_eq!(compute_style(&el("span", &[]), &sheet, None).display, Display::Inline);
        assert_eq!(compute_style(&el("head", &[]), &sheet, None).display, Display::None);
        assert_eq!(compute_style(&el("h1", &[]), &sheet, None).font_size, 32.0);
        let a = compute_style(&el("a", &[]), &sheet, None);
        assert_eq!(a.color, Color::BLUE);
        assert_eq!(a.text_decoration, TextDecoration::Underline);
    }

    #[test]
    fn specificity_beats_order() {
        let sheet = CssParser::parse("#x { color: red } .c { color: green } p { color: blue }");
        let s = compute_style(&el("p", &[("id", "x"), ("class", "c")]), &sheet, None);
        assert_eq!(s.color, Color::from_name("red").unwrap());
        let s = compute_style(&el("p", &[("class", "c")]), &sheet, None);
        assert_eq!(s.color, Color::from_name("green").unwrap());
    }

    #[test]
    fn later_wins_at_equal_specificity() {
        let sheet = CssParser::parse("p { color: red } p { color: blue }");
        let s = compute_style(&el("p", &[]), &sheet, None);
        assert_eq!(s.color, Color::from_name("blue").unwrap());
    }

    #[test]
    fn inherited_and_not_inherited() {
        let sheet = CssParser::parse("div { color: red; background-color: yellow; font-size: 20px }");
        let parent = compute_style(&el("div", &[]), &sheet, None);
        let child = compute_style(&el("span", &[]), &sheet, Some(&parent));
        assert_eq!(child.color, parent.color); // color は継承する
        assert_eq!(child.font_size, 20.0); // font-size も継承する
        assert_eq!(child.background_color, None); // background-color は継承しない
    }

    #[test]
    fn author_beats_user_agent_but_inherits_otherwise() {
        let sheet = CssParser::parse("a { color: black }");
        let a = compute_style(&el("a", &[]), &sheet, None);
        assert_eq!(a.color, Color::BLACK); // 作者の宣言が UA の青に勝つ
        assert_eq!(a.text_decoration, TextDecoration::Underline); // 宣言していないものは UA のまま
    }

    #[test]
    fn hex_colors() {
        assert_eq!(Color::from_hex("ff0000"), Some(Color { r: 255, g: 0, b: 0 }));
        assert_eq!(Color::from_hex("f00"), Some(Color { r: 255, g: 0, b: 0 }));
        assert_eq!(Color::from_hex("zz"), None);
    }
}
