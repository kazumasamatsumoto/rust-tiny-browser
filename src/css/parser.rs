//! CSS の構文解析(CSS Syntax Module Level 3 §5 Parsing)。トークン列 → CSSOM。
//!
//! 仕様の芯は「壊れていても止まらない」こと。読めないルールや宣言は、
//! 「通常の解析に戻れる最小限だけ捨てて」次へ進む。ここでも読めない宣言は飛ばし、
//! 読めないセレクタは Unsupported として残す(どの要素にも当たらない)。

use std::iter::Peekable;

use crate::css::cssom::{Declaration, QualifiedRule, Selector, StyleSheet};
use crate::css::token::CssToken;
use crate::css::tokenizer::CssTokenizer;

pub struct CssParser {
    tokens: Peekable<CssTokenizer>,
}

impl CssParser {
    pub fn new(css: &str) -> Self {
        CssParser {
            tokens: CssTokenizer::new(css).peekable(),
        }
    }

    /// 文字列から StyleSheet を作る
    pub fn parse(css: &str) -> StyleSheet {
        CssParser::new(css).parse_stylesheet()
    }

    /// 仕様 "parse a stylesheet" → "consume a list of rules"
    pub fn parse_stylesheet(mut self) -> StyleSheet {
        let mut rules = Vec::new();
        loop {
            self.skip_whitespace();
            match self.tokens.peek() {
                None => break,
                Some(CssToken::AtKeyword(_)) => self.skip_at_rule(),
                Some(_) => {
                    if let Some(rule) = self.consume_qualified_rule() {
                        rules.push(rule);
                    }
                }
            }
        }
        StyleSheet { rules }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.tokens.peek(), Some(CssToken::Whitespace)) {
            self.tokens.next();
        }
    }

    /// 仕様 "consume a qualified rule": "{" までが prelude、"{ ... }" が宣言のブロック。
    /// "{" の前に入力が終わったら、そのルールは捨てる(parse error)
    fn consume_qualified_rule(&mut self) -> Option<QualifiedRule> {
        let mut prelude = Vec::new();
        loop {
            match self.tokens.next() {
                None => return None,
                Some(CssToken::OpenCurly) => break,
                Some(t) => prelude.push(t),
            }
        }
        let selectors = parse_selector_list(&prelude);
        let declarations = self.consume_declarations();
        Some(QualifiedRule {
            selectors,
            declarations,
        })
    }

    /// 仕様 "consume a list of declarations"(簡略版)。"}" か入力の終わりまで
    fn consume_declarations(&mut self) -> Vec<Declaration> {
        let mut decls = Vec::new();
        loop {
            self.skip_whitespace();
            match self.tokens.next() {
                None | Some(CssToken::CloseCurly) => break,
                Some(CssToken::Semicolon) => continue,
                Some(CssToken::Ident(name)) => {
                    self.skip_whitespace();
                    if !matches!(self.tokens.peek(), Some(CssToken::Colon)) {
                        // "color red;" のように ":" が無い → この宣言は捨てる
                        self.skip_to_end_of_declaration();
                        continue;
                    }
                    self.tokens.next(); // ":"
                    let mut value = Vec::new();
                    loop {
                        match self.tokens.peek() {
                            None | Some(CssToken::Semicolon) | Some(CssToken::CloseCurly) => break,
                            Some(_) => value.push(self.tokens.next().unwrap()),
                        }
                    }
                    // 前後の空白を除く
                    while matches!(value.first(), Some(CssToken::Whitespace)) {
                        value.remove(0);
                    }
                    while matches!(value.last(), Some(CssToken::Whitespace)) {
                        value.pop();
                    }
                    if !value.is_empty() {
                        decls.push(Declaration {
                            property: name,
                            value,
                        });
                    }
                }
                Some(_) => self.skip_to_end_of_declaration(), // 名前が識別子でない → 捨てる
            }
        }
        decls
    }

    /// 次の ";" まで読み捨てる。"}" は消費しない(ブロックの終わりなので)
    fn skip_to_end_of_declaration(&mut self) {
        loop {
            match self.tokens.peek() {
                None | Some(CssToken::CloseCurly) => return,
                Some(CssToken::Semicolon) => {
                    self.tokens.next();
                    return;
                }
                Some(_) => {
                    self.tokens.next();
                }
            }
        }
    }

    /// @media などの at-rule を丸ごと読み捨てる。";" で終わるか、{} のブロックが閉じるまで
    fn skip_at_rule(&mut self) {
        let mut depth = 0usize;
        for t in self.tokens.by_ref() {
            match t {
                CssToken::Semicolon if depth == 0 => return,
                CssToken::OpenCurly => depth += 1,
                CssToken::CloseCurly => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }
}

/// prelude をセレクタのリストに読み替える。コンマで区切り、各区切りは単純セレクタ 1 つだけを受け付ける
fn parse_selector_list(prelude: &[CssToken]) -> Vec<Selector> {
    prelude
        .split(|t| *t == CssToken::Comma)
        .map(|group| {
            let parts: Vec<&CssToken> = group.iter().filter(|t| **t != CssToken::Whitespace).collect();
            match parts.as_slice() {
                [CssToken::Ident(tag)] => Selector::Type(tag.to_ascii_lowercase()),
                [CssToken::Hash(id)] => Selector::Id(id.clone()),
                [CssToken::Delim('*')] => Selector::Universal,
                [CssToken::Delim('.'), CssToken::Ident(class)] => Selector::Class(class.clone()),
                _ => Selector::Unsupported(parts.iter().map(|t| t.to_css()).collect::<Vec<_>>().join("")),
            }
        })
        .filter(|s| !matches!(s, Selector::Unsupported(text) if text.is_empty()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_stylesheet() {
        assert_eq!(CssParser::parse("").rules.len(), 0);
    }

    #[test]
    fn one_rule_two_declarations() {
        let sheet = CssParser::parse("p { color: red; font-size: 16px }");
        assert_eq!(sheet.rules.len(), 1);
        let r = &sheet.rules[0];
        assert_eq!(r.selectors, vec![Selector::Type("p".into())]);
        assert_eq!(r.declarations.len(), 2);
        assert_eq!(r.declarations[0].property, "color");
        assert_eq!(r.declarations[0].value, vec![CssToken::Ident("red".into())]);
        assert_eq!(r.declarations[1].value, vec![CssToken::Dimension(16.0, "px".into())]);
    }

    #[test]
    fn id_class_universal_and_list() {
        let sheet = CssParser::parse("#main { } .note { } * { } h1, h2 { }");
        let sels: Vec<Selector> = sheet.rules.iter().flat_map(|r| r.selectors.clone()).collect();
        assert_eq!(
            sels,
            vec![
                Selector::Id("main".into()),
                Selector::Class("note".into()),
                Selector::Universal,
                Selector::Type("h1".into()),
                Selector::Type("h2".into()),
            ]
        );
    }

    #[test]
    fn unsupported_selector_is_kept_but_never_matches() {
        let sheet = CssParser::parse("div p { color: red }");
        assert_eq!(sheet.rules[0].selectors, vec![Selector::Unsupported("divp".into())]);
    }

    #[test]
    fn broken_declaration_is_skipped_but_rule_survives() {
        let sheet = CssParser::parse("p { color red; background-color: blue }");
        let r = &sheet.rules[0];
        assert_eq!(r.declarations.len(), 1);
        assert_eq!(r.declarations[0].property, "background-color");
    }

    #[test]
    fn at_rule_and_comment_are_skipped() {
        let sheet = CssParser::parse("@charset \"utf-8\"; @media (max-width: 600px) { p { color: red } } /* c */ a { color: blue }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].selectors, vec![Selector::Type("a".into())]);
    }

    #[test]
    fn specificity_orders_id_class_type() {
        assert!(Selector::Id("x".into()).specificity() > Selector::Class("x".into()).specificity());
        assert!(Selector::Class("x".into()).specificity() > Selector::Type("p".into()).specificity());
        assert!(Selector::Type("p".into()).specificity() > Selector::Universal.specificity());
    }
}
