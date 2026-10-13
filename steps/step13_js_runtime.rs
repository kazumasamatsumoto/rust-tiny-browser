// ステップ 13: AST を評価する(環境・スコープ・関数)
//
// このファイル 1 つで動きます:
//   実行:      rustc --edition 2021 step13_js_runtime.rs && ./step13_js_runtime
//   テスト:    rustc --edition 2021 --test step13_js_runtime.rs -o step13_js_runtime_test && ./step13_js_runtime_test
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
    }
    pub mod ast {
        //! 抽象構文木(AST)。構文解析の出力で、評価の入力。
        //!
        //! 名前は ECMAScript の文法の非終端記号に合わせてある:
        //! VariableStatement / FunctionDeclaration / ReturnStatement / ExpressionStatement /
        //! AssignmentExpression / AdditiveExpression(ここでは BinaryExpression)/ MemberExpression / CallExpression /
        //! PrimaryExpression(Identifier・NumericLiteral・StringLiteral)。

        #[derive(Debug, Clone, PartialEq)]
        pub struct Program {
            pub body: Vec<Node>,
        }

        #[derive(Debug, Clone, PartialEq)]
        pub enum Node {
            // ── 文 ──
            /// var x = 1;  (VariableStatement : var VariableDeclarationList ;)
            VariableDeclaration { name: String, init: Option<Box<Node>> },
            /// function f(a, b) { ... }
            FunctionDeclaration { name: String, params: Vec<String>, body: Vec<Node> },
            /// return expr;
            ReturnStatement(Option<Box<Node>>),
            /// 式だけの文。  f(); や x = 1;
            ExpressionStatement(Box<Node>),

            // ── 式 ──
            /// target = value
            AssignmentExpression { target: Box<Node>, value: Box<Node> },
            /// left + right / left - right  (AdditiveExpression)
            BinaryExpression { operator: char, left: Box<Node>, right: Box<Node> },
            /// object.property  (MemberExpression : MemberExpression . IdentifierName)
            MemberExpression { object: Box<Node>, property: String },
            /// callee(arguments)  (CallExpression : MemberExpression Arguments)
            CallExpression { callee: Box<Node>, arguments: Vec<Node> },
            Identifier(String),
            NumericLiteral(f64),
            StringLiteral(String),
        }

        impl Node {
            /// 木を字下げして表示する(学習用)
            pub fn dump(&self) -> String {
                let mut out = String::new();
                self.dump_into(0, &mut out);
                out
            }

            fn dump_into(&self, depth: usize, out: &mut String) {
                use std::fmt::Write as _;
                let indent = "  ".repeat(depth);
                match self {
                    Node::VariableDeclaration { name, init } => {
                        writeln!(out, "{indent}VariableDeclaration name={name}").unwrap();
                        if let Some(i) = init {
                            i.dump_into(depth + 1, out);
                        }
                    }
                    Node::FunctionDeclaration { name, params, body } => {
                        writeln!(out, "{indent}FunctionDeclaration name={name} params={params:?}").unwrap();
                        for s in body {
                            s.dump_into(depth + 1, out);
                        }
                    }
                    Node::ReturnStatement(e) => {
                        writeln!(out, "{indent}ReturnStatement").unwrap();
                        if let Some(e) = e {
                            e.dump_into(depth + 1, out);
                        }
                    }
                    Node::ExpressionStatement(e) => {
                        writeln!(out, "{indent}ExpressionStatement").unwrap();
                        e.dump_into(depth + 1, out);
                    }
                    Node::AssignmentExpression { target, value } => {
                        writeln!(out, "{indent}AssignmentExpression").unwrap();
                        target.dump_into(depth + 1, out);
                        value.dump_into(depth + 1, out);
                    }
                    Node::BinaryExpression { operator, left, right } => {
                        writeln!(out, "{indent}BinaryExpression {operator}").unwrap();
                        left.dump_into(depth + 1, out);
                        right.dump_into(depth + 1, out);
                    }
                    Node::MemberExpression { object, property } => {
                        writeln!(out, "{indent}MemberExpression .{property}").unwrap();
                        object.dump_into(depth + 1, out);
                    }
                    Node::CallExpression { callee, arguments } => {
                        writeln!(out, "{indent}CallExpression").unwrap();
                        callee.dump_into(depth + 1, out);
                        for a in arguments {
                            a.dump_into(depth + 1, out);
                        }
                    }
                    Node::Identifier(n) => writeln!(out, "{indent}Identifier {n}").unwrap(),
                    Node::NumericLiteral(v) => writeln!(out, "{indent}NumericLiteral {v}").unwrap(),
                    Node::StringLiteral(s) => writeln!(out, "{indent}StringLiteral {s:?}").unwrap(),
                }
            }
        }
    }
    pub mod parser {
        //! JavaScript の構文解析。トークン列 → AST。
        //!
        //! ECMAScript の文法を、必要な生成規則だけ「再帰下降」で書く。関数 1 つが生成規則 1 つに対応する:
        //!
        //! ```text
        //! Program               : Statement*
        //! Statement             : VariableStatement | FunctionDeclaration | ReturnStatement | ExpressionStatement
        //! VariableStatement     : var BindingIdentifier ( = AssignmentExpression )? ;
        //! FunctionDeclaration   : function BindingIdentifier ( FormalParameters ) { FunctionBody }
        //! ReturnStatement       : return Expression? ;
        //! AssignmentExpression  : LeftHandSideExpression = AssignmentExpression | AdditiveExpression
        //! AdditiveExpression    : AdditiveExpression + LeftHandSideExpression | AdditiveExpression - ... | LeftHandSideExpression
        //! LeftHandSideExpression: PrimaryExpression ( . IdentifierName | Arguments )*      ← MemberExpression と CallExpression
        //! PrimaryExpression     : Identifier | NumericLiteral | StringLiteral | ( Expression )
        //! ```
        //!
        //! 本物の AdditiveExpression は MultiplicativeExpression を間に挟む(* / % の優先順位のため)。
        //! この実装は * / を扱わないので、その段を飛ばしている。

        use std::fmt;

        use crate::js::ast::{Node, Program};
        use crate::js::lexer::{JsLexer, Token};

        #[derive(Debug, Clone, PartialEq)]
        pub enum ParseError {
            UnexpectedToken { expected: String, found: Token },
            UnexpectedEnd { expected: String },
        }

        impl fmt::Display for ParseError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    ParseError::UnexpectedToken { expected, found } => {
                        write!(f, "SyntaxError: {expected} を期待しましたが {found:?} が来ました")
                    }
                    ParseError::UnexpectedEnd { expected } => {
                        write!(f, "SyntaxError: {expected} を期待しましたが入力が終わりました")
                    }
                }
            }
        }

        pub struct JsParser {
            tokens: Vec<Token>,
            pos: usize,
        }

        impl JsParser {
            pub fn new(src: &str) -> Self {
                JsParser {
                    tokens: JsLexer::new(src).collect(),
                    pos: 0,
                }
            }

            pub fn parse(src: &str) -> Result<Program, ParseError> {
                JsParser::new(src).parse_program()
            }

            fn peek(&self) -> Option<&Token> {
                self.tokens.get(self.pos)
            }

            fn next(&mut self) -> Option<Token> {
                let t = self.tokens.get(self.pos).cloned();
                if t.is_some() {
                    self.pos += 1;
                }
                t
            }

            fn is_punct(&self, c: char) -> bool {
                matches!(self.peek(), Some(Token::Punctuator(p)) if *p == c)
            }

            fn is_keyword(&self, k: &str) -> bool {
                matches!(self.peek(), Some(Token::Keyword(w)) if w == k)
            }

            /// 期待する記号を 1 つ消費する
            fn expect_punct(&mut self, c: char) -> Result<(), ParseError> {
                match self.next() {
                    Some(Token::Punctuator(p)) if p == c => Ok(()),
                    Some(found) => Err(ParseError::UnexpectedToken {
                        expected: format!("'{c}'"),
                        found,
                    }),
                    None => Err(ParseError::UnexpectedEnd {
                        expected: format!("'{c}'"),
                    }),
                }
            }

            fn expect_identifier(&mut self) -> Result<String, ParseError> {
                match self.next() {
                    Some(Token::Identifier(name)) => Ok(name),
                    Some(found) => Err(ParseError::UnexpectedToken {
                        expected: "識別子".into(),
                        found,
                    }),
                    None => Err(ParseError::UnexpectedEnd {
                        expected: "識別子".into(),
                    }),
                }
            }

            /// 文末の ";" は省略できる(本物の自動セミコロン挿入の、ごく粗い代用)
            fn optional_semicolon(&mut self) {
                if self.is_punct(';') {
                    self.pos += 1;
                }
            }

            // Program : Statement*
            pub fn parse_program(&mut self) -> Result<Program, ParseError> {
                let mut body = Vec::new();
                while self.peek().is_some() {
                    body.push(self.parse_statement()?);
                }
                Ok(Program { body })
            }

            // Statement : VariableStatement | FunctionDeclaration | ReturnStatement | ExpressionStatement
            fn parse_statement(&mut self) -> Result<Node, ParseError> {
                if self.is_keyword("var") || self.is_keyword("let") || self.is_keyword("const") {
                    self.pos += 1;
                    return self.parse_variable_declaration();
                }
                if self.is_keyword("function") {
                    self.pos += 1;
                    return self.parse_function_declaration();
                }
                if self.is_keyword("return") {
                    self.pos += 1;
                    let value = if self.is_punct(';') || self.is_punct('}') || self.peek().is_none() {
                        None
                    } else {
                        Some(Box::new(self.parse_assignment_expression()?))
                    };
                    self.optional_semicolon();
                    return Ok(Node::ReturnStatement(value));
                }
                let expr = self.parse_assignment_expression()?;
                self.optional_semicolon();
                Ok(Node::ExpressionStatement(Box::new(expr)))
            }

            // VariableDeclaration : BindingIdentifier Initializer?
            fn parse_variable_declaration(&mut self) -> Result<Node, ParseError> {
                let name = self.expect_identifier()?;
                let init = if self.is_punct('=') {
                    self.pos += 1;
                    Some(Box::new(self.parse_assignment_expression()?))
                } else {
                    None
                };
                self.optional_semicolon();
                Ok(Node::VariableDeclaration { name, init })
            }

            // FunctionDeclaration : function BindingIdentifier ( FormalParameters ) { FunctionBody }
            fn parse_function_declaration(&mut self) -> Result<Node, ParseError> {
                let name = self.expect_identifier()?;
                self.expect_punct('(')?;
                let mut params = Vec::new();
                while !self.is_punct(')') {
                    params.push(self.expect_identifier()?);
                    if self.is_punct(',') {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                self.expect_punct(')')?;
                self.expect_punct('{')?;
                let mut body = Vec::new();
                while !self.is_punct('}') {
                    if self.peek().is_none() {
                        return Err(ParseError::UnexpectedEnd { expected: "'}'".into() });
                    }
                    body.push(self.parse_statement()?);
                }
                self.expect_punct('}')?;
                Ok(Node::FunctionDeclaration { name, params, body })
            }

            // AssignmentExpression : LeftHandSideExpression = AssignmentExpression | AdditiveExpression
            fn parse_assignment_expression(&mut self) -> Result<Node, ParseError> {
                let left = self.parse_additive_expression()?;
                if self.is_punct('=') {
                    self.pos += 1;
                    let value = self.parse_assignment_expression()?; // 右結合: a = b = 1
                    return Ok(Node::AssignmentExpression {
                        target: Box::new(left),
                        value: Box::new(value),
                    });
                }
                Ok(left)
            }

            // AdditiveExpression : AdditiveExpression + ... | AdditiveExpression - ... | ...
            // 左再帰はループに書き換える(左結合: 1 - 2 - 3 = (1 - 2) - 3)
            fn parse_additive_expression(&mut self) -> Result<Node, ParseError> {
                let mut left = self.parse_left_hand_side_expression()?;
                while self.is_punct('+') || self.is_punct('-') {
                    let operator = match self.next() {
                        Some(Token::Punctuator(c)) => c,
                        _ => unreachable!(),
                    };
                    let right = self.parse_left_hand_side_expression()?;
                    left = Node::BinaryExpression {
                        operator,
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                Ok(left)
            }

            // LeftHandSideExpression : MemberExpression | CallExpression
            // MemberExpression : PrimaryExpression | MemberExpression . IdentifierName
            // CallExpression   : MemberExpression Arguments | CallExpression Arguments | CallExpression . IdentifierName
            fn parse_left_hand_side_expression(&mut self) -> Result<Node, ParseError> {
                let mut expr = self.parse_primary_expression()?;
                loop {
                    if self.is_punct('.') {
                        self.pos += 1;
                        let property = self.expect_identifier()?;
                        expr = Node::MemberExpression {
                            object: Box::new(expr),
                            property,
                        };
                    } else if self.is_punct('(') {
                        self.pos += 1;
                        let mut arguments = Vec::new();
                        while !self.is_punct(')') {
                            arguments.push(self.parse_assignment_expression()?);
                            if self.is_punct(',') {
                                self.pos += 1;
                            } else {
                                break;
                            }
                        }
                        self.expect_punct(')')?;
                        expr = Node::CallExpression {
                            callee: Box::new(expr),
                            arguments,
                        };
                    } else {
                        return Ok(expr);
                    }
                }
            }

            // PrimaryExpression : IdentifierReference | Literal | ( Expression )
            fn parse_primary_expression(&mut self) -> Result<Node, ParseError> {
                match self.next() {
                    Some(Token::Number(n)) => Ok(Node::NumericLiteral(n)),
                    Some(Token::Str(s)) => Ok(Node::StringLiteral(s)),
                    Some(Token::Identifier(name)) => Ok(Node::Identifier(name)),
                    Some(Token::Punctuator('(')) => {
                        let e = self.parse_assignment_expression()?;
                        self.expect_punct(')')?;
                        Ok(e)
                    }
                    Some(found) => Err(ParseError::UnexpectedToken {
                        expected: "式".into(),
                        found,
                    }),
                    None => Err(ParseError::UnexpectedEnd { expected: "式".into() }),
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            fn num(n: f64) -> Box<Node> {
                Box::new(Node::NumericLiteral(n))
            }

            #[test]
            fn empty_program() {
                assert_eq!(JsParser::parse("").unwrap().body.len(), 0);
            }

            #[test]
            fn addition_is_left_associative() {
                let p = JsParser::parse("1 + 2 - 3").unwrap();
                assert_eq!(
                    p.body[0],
                    Node::ExpressionStatement(Box::new(Node::BinaryExpression {
                        operator: '-',
                        left: Box::new(Node::BinaryExpression {
                            operator: '+',
                            left: num(1.0),
                            right: num(2.0),
                        }),
                        right: num(3.0),
                    }))
                );
            }

            #[test]
            fn variable_and_assignment() {
                let p = JsParser::parse("var x = 1; x = x + 1;").unwrap();
                assert_eq!(
                    p.body[0],
                    Node::VariableDeclaration {
                        name: "x".into(),
                        init: Some(num(1.0))
                    }
                );
                assert!(matches!(p.body[1], Node::ExpressionStatement(ref e) if matches!(**e, Node::AssignmentExpression { .. })));
            }

            #[test]
            fn function_declaration_and_call() {
                let p = JsParser::parse("function add(a, b) { return a + b; } add(1, 2)").unwrap();
                match &p.body[0] {
                    Node::FunctionDeclaration { name, params, body } => {
                        assert_eq!(name, "add");
                        assert_eq!(params, &vec!["a".to_string(), "b".to_string()]);
                        assert_eq!(body.len(), 1);
                    }
                    other => panic!("{other:?}"),
                }
                match &p.body[1] {
                    Node::ExpressionStatement(e) => match &**e {
                        Node::CallExpression { callee, arguments } => {
                            assert_eq!(**callee, Node::Identifier("add".into()));
                            assert_eq!(arguments.len(), 2);
                        }
                        other => panic!("{other:?}"),
                    },
                    other => panic!("{other:?}"),
                }
            }

            #[test]
            fn member_then_call() {
                let p = JsParser::parse("document.getElementById('t').textContent = 'x'").unwrap();
                match &p.body[0] {
                    Node::ExpressionStatement(e) => match &**e {
                        Node::AssignmentExpression { target, .. } => match &**target {
                            Node::MemberExpression { object, property } => {
                                assert_eq!(property, "textContent");
                                assert!(matches!(**object, Node::CallExpression { .. }));
                            }
                            other => panic!("{other:?}"),
                        },
                        other => panic!("{other:?}"),
                    },
                    other => panic!("{other:?}"),
                }
            }

            #[test]
            fn syntax_errors() {
                assert!(matches!(JsParser::parse("var = 1"), Err(ParseError::UnexpectedToken { .. })));
                assert!(matches!(JsParser::parse("f("), Err(ParseError::UnexpectedEnd { .. })));
                assert!(matches!(JsParser::parse("1 +"), Err(ParseError::UnexpectedEnd { .. })));
            }
        }
    }
    pub mod runtime {
        //! JavaScript の評価(インタプリタ)。AST を根から辿って値を計算する。
        //!
        //! 持っているもの:
        //! - 値: undefined / null / 数値 / 文字列 / 関数 / 組み込み関数 / オブジェクト(document・console・要素)
        //! - 環境(Environment): 名前 → 値 の表と、外側の環境への参照。関数を呼ぶたびに新しい環境を作り、
        //!   見つからなければ外側へ辿る(スコープチェーン)
        //! - ブラウザ API: document.getElementById(id)、要素.textContent の読み書き、console.log(...)
        //!
        //! 持っていないもの: if / for / オブジェクトリテラル / 配列 / this / プロトタイプ / 例外 / イベント / 非同期。

        use std::cell::RefCell;
        use std::collections::HashMap;
        use std::fmt;
        use std::rc::Rc;

        use crate::html::dom::{Node as DomNode, NodeRef};
        use crate::js::ast::{Node, Program};
        use crate::js::parser::{JsParser, ParseError};

        pub type EnvRef = Rc<RefCell<Environment>>;

        #[derive(Debug, Clone)]
        pub enum RuntimeValue {
            Undefined,
            Null,
            Number(f64),
            Str(String),
            /// ユーザー定義の関数。定義されたときの環境を閉じ込める(クロージャ)
            Function {
                params: Vec<String>,
                body: Vec<Node>,
                closure: EnvRef,
            },
            /// Rust 側で実装した関数
            Builtin(Builtin),
            /// オブジェクト。ブラウザ API のために最小限だけ
            Object(JsObject),
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Builtin {
            ConsoleLog,
            GetElementById,
        }

        #[derive(Debug, Clone)]
        pub enum JsObject {
            Document,
            Console,
            Element(NodeRef),
        }

        impl PartialEq for RuntimeValue {
            fn eq(&self, other: &Self) -> bool {
                match (self, other) {
                    (RuntimeValue::Undefined, RuntimeValue::Undefined) => true,
                    (RuntimeValue::Null, RuntimeValue::Null) => true,
                    (RuntimeValue::Number(a), RuntimeValue::Number(b)) => a == b,
                    (RuntimeValue::Str(a), RuntimeValue::Str(b)) => a == b,
                    (RuntimeValue::Builtin(a), RuntimeValue::Builtin(b)) => a == b,
                    _ => false,
                }
            }
        }

        /// JavaScript の ToString に近い表示
        impl fmt::Display for RuntimeValue {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    RuntimeValue::Undefined => write!(f, "undefined"),
                    RuntimeValue::Null => write!(f, "null"),
                    RuntimeValue::Number(n) => {
                        if n.fract() == 0.0 && n.abs() < 1e15 {
                            write!(f, "{}", *n as i64)
                        } else {
                            write!(f, "{n}")
                        }
                    }
                    RuntimeValue::Str(s) => write!(f, "{s}"),
                    RuntimeValue::Function { params, .. } => write!(f, "function({})", params.join(", ")),
                    RuntimeValue::Builtin(b) => write!(f, "function {b:?}() {{ [native code] }}"),
                    RuntimeValue::Object(JsObject::Document) => write!(f, "[object HTMLDocument]"),
                    RuntimeValue::Object(JsObject::Console) => write!(f, "[object console]"),
                    RuntimeValue::Object(JsObject::Element(n)) => {
                        write!(f, "[object HTMLElement <{}>]", n.tag().unwrap_or("?"))
                    }
                }
            }
        }

        #[derive(Debug, Clone, PartialEq)]
        pub enum JsError {
            Syntax(ParseError),
            ReferenceError(String),
            TypeError(String),
        }

        impl fmt::Display for JsError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    JsError::Syntax(e) => write!(f, "{e}"),
                    JsError::ReferenceError(m) => write!(f, "ReferenceError: {m}"),
                    JsError::TypeError(m) => write!(f, "TypeError: {m}"),
                }
            }
        }

        /// 変数の表。外側(outer)を辿れる
        #[derive(Debug, Default)]
        pub struct Environment {
            values: HashMap<String, RuntimeValue>,
            outer: Option<EnvRef>,
        }

        impl Environment {
            pub fn new(outer: Option<EnvRef>) -> EnvRef {
                Rc::new(RefCell::new(Environment {
                    values: HashMap::new(),
                    outer,
                }))
            }

            /// 名前を探す。無ければ外側へ(スコープチェーン)
            pub fn get(&self, name: &str) -> Option<RuntimeValue> {
                match self.values.get(name) {
                    Some(v) => Some(v.clone()),
                    None => self.outer.as_ref().and_then(|o| o.borrow().get(name)),
                }
            }

            /// この環境に新しく定義する(var / function / 仮引数)
            pub fn define(&mut self, name: &str, value: RuntimeValue) {
                self.values.insert(name.to_string(), value);
            }

            /// 既存の変数に代入する。見つかった環境で書き換える。どこにも無ければ false
            pub fn assign(&mut self, name: &str, value: RuntimeValue) -> bool {
                if self.values.contains_key(name) {
                    self.values.insert(name.to_string(), value);
                    return true;
                }
                match &self.outer {
                    Some(o) => o.borrow_mut().assign(name, value),
                    None => false,
                }
            }
        }

        /// 文の実行結果。return が起きたら呼び出し元まで持ち上げる
        enum Completion {
            Normal(RuntimeValue),
            Return(RuntimeValue),
        }

        pub struct JsRuntime {
            pub global: EnvRef,
            document: Option<NodeRef>,
            /// console.log の出力
            pub console_output: Vec<String>,
            /// textContent への代入などで DOM が変わったか(変わったら再レイアウトが要る)
            pub dom_changed: bool,
        }

        impl JsRuntime {
            /// document を渡すとブラウザ API が使える。None なら素の JavaScript だけ
            pub fn new(document: Option<NodeRef>) -> Self {
                let global = Environment::new(None);
                global.borrow_mut().define("console", RuntimeValue::Object(JsObject::Console));
                if document.is_some() {
                    global.borrow_mut().define("document", RuntimeValue::Object(JsObject::Document));
                }
                JsRuntime {
                    global,
                    document,
                    console_output: Vec::new(),
                    dom_changed: false,
                }
            }

            /// ソースを解析して実行し、最後に評価した値を返す
            pub fn run(&mut self, source: &str) -> Result<RuntimeValue, JsError> {
                let program: Program = JsParser::parse(source).map_err(JsError::Syntax)?;
                let env = self.global.clone();
                match self.exec_block(&program.body, &env)? {
                    Completion::Normal(v) | Completion::Return(v) => Ok(v),
                }
            }

            fn exec_block(&mut self, statements: &[Node], env: &EnvRef) -> Result<Completion, JsError> {
                // 関数宣言は巻き上げ(hoisting)される: ブロックの先頭で先に定義する
                for s in statements {
                    if let Node::FunctionDeclaration { name, params, body } = s {
                        env.borrow_mut().define(
                            name,
                            RuntimeValue::Function {
                                params: params.clone(),
                                body: body.clone(),
                                closure: env.clone(),
                            },
                        );
                    }
                }
                let mut last = RuntimeValue::Undefined;
                for s in statements {
                    match self.exec_statement(s, env)? {
                        // 関数宣言と変数宣言は「値を持たない文」。最後の値を上書きしない(ECMAScript の empty completion)
                        Completion::Normal(_) if matches!(s, Node::FunctionDeclaration { .. } | Node::VariableDeclaration { .. }) => {}
                        Completion::Normal(v) => last = v,
                        ret @ Completion::Return(_) => return Ok(ret),
                    }
                }
                Ok(Completion::Normal(last))
            }

            fn exec_statement(&mut self, s: &Node, env: &EnvRef) -> Result<Completion, JsError> {
                match s {
                    Node::VariableDeclaration { name, init } => {
                        let v = match init {
                            Some(e) => self.eval(e, env)?,
                            None => RuntimeValue::Undefined,
                        };
                        env.borrow_mut().define(name, v);
                        Ok(Completion::Normal(RuntimeValue::Undefined))
                    }
                    Node::FunctionDeclaration { .. } => Ok(Completion::Normal(RuntimeValue::Undefined)), // 巻き上げ済み
                    Node::ReturnStatement(e) => {
                        let v = match e {
                            Some(e) => self.eval(e, env)?,
                            None => RuntimeValue::Undefined,
                        };
                        Ok(Completion::Return(v))
                    }
                    Node::ExpressionStatement(e) => Ok(Completion::Normal(self.eval(e, env)?)),
                    other => Ok(Completion::Normal(self.eval(other, env)?)),
                }
            }

            /// 式を評価して値にする
            pub fn eval(&mut self, node: &Node, env: &EnvRef) -> Result<RuntimeValue, JsError> {
                match node {
                    Node::NumericLiteral(n) => Ok(RuntimeValue::Number(*n)),
                    Node::StringLiteral(s) => Ok(RuntimeValue::Str(s.clone())),
                    Node::Identifier(name) => env
                        .borrow()
                        .get(name)
                        .ok_or_else(|| JsError::ReferenceError(format!("{name} is not defined"))),

                    Node::BinaryExpression { operator, left, right } => {
                        let l = self.eval(left, env)?;
                        let r = self.eval(right, env)?;
                        binary(*operator, l, r)
                    }

                    Node::AssignmentExpression { target, value } => {
                        let v = self.eval(value, env)?;
                        match &**target {
                            Node::Identifier(name) => {
                                if !env.borrow_mut().assign(name, v.clone()) {
                                    // 宣言していない名前への代入は、(非 strict の JS と同じく)グローバルに作る
                                    self.global.borrow_mut().define(name, v.clone());
                                }
                                Ok(v)
                            }
                            Node::MemberExpression { object, property } => {
                                let obj = self.eval(object, env)?;
                                match (obj, property.as_str()) {
                                    (RuntimeValue::Object(JsObject::Element(node)), "textContent") => {
                                        DomNode::set_text_content(&node, &v.to_string());
                                        self.dom_changed = true;
                                        Ok(v)
                                    }
                                    (obj, prop) => Err(JsError::TypeError(format!("{obj} の {prop} には代入できません"))),
                                }
                            }
                            other => Err(JsError::TypeError(format!("代入先にできません: {other:?}"))),
                        }
                    }

                    Node::MemberExpression { object, property } => {
                        let obj = self.eval(object, env)?;
                        Ok(match (&obj, property.as_str()) {
                            (RuntimeValue::Object(JsObject::Document), "getElementById") => {
                                RuntimeValue::Builtin(Builtin::GetElementById)
                            }
                            (RuntimeValue::Object(JsObject::Console), "log") => RuntimeValue::Builtin(Builtin::ConsoleLog),
                            (RuntimeValue::Object(JsObject::Element(n)), "textContent") => RuntimeValue::Str(n.text_content()),
                            (RuntimeValue::Object(JsObject::Element(n)), "id") => {
                                RuntimeValue::Str(n.as_element().and_then(|e| e.id()).unwrap_or("").to_string())
                            }
                            (RuntimeValue::Object(JsObject::Element(n)), "tagName") => {
                                RuntimeValue::Str(n.tag().unwrap_or("").to_ascii_uppercase())
                            }
                            (RuntimeValue::Str(s), "length") => RuntimeValue::Number(s.chars().count() as f64),
                            (RuntimeValue::Undefined, p) | (RuntimeValue::Null, p) => {
                                return Err(JsError::TypeError(format!(
                                    "Cannot read properties of {obj} (reading '{p}')"
                                )))
                            }
                            _ => RuntimeValue::Undefined, // 知らないプロパティは undefined
                        })
                    }

                    Node::CallExpression { callee, arguments } => {
                        let f = self.eval(callee, env)?;
                        let mut args = Vec::new();
                        for a in arguments {
                            args.push(self.eval(a, env)?);
                        }
                        self.call(f, args)
                    }

                    // 文が式の位置に来ることはないが、網羅のために
                    other => match self.exec_statement(other, env)? {
                        Completion::Normal(v) | Completion::Return(v) => Ok(v),
                    },
                }
            }

            fn call(&mut self, f: RuntimeValue, args: Vec<RuntimeValue>) -> Result<RuntimeValue, JsError> {
                match f {
                    RuntimeValue::Function { params, body, closure } => {
                        // 呼び出しごとに新しい環境。外側は「定義されたときの環境」(字句スコープ)
                        let local = Environment::new(Some(closure));
                        for (i, p) in params.iter().enumerate() {
                            let v = args.get(i).cloned().unwrap_or(RuntimeValue::Undefined);
                            local.borrow_mut().define(p, v);
                        }
                        match self.exec_block(&body, &local)? {
                            Completion::Return(v) => Ok(v),
                            Completion::Normal(_) => Ok(RuntimeValue::Undefined), // return が無ければ undefined
                        }
                    }
                    RuntimeValue::Builtin(Builtin::ConsoleLog) => {
                        let line = args.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(" ");
                        self.console_output.push(line);
                        Ok(RuntimeValue::Undefined)
                    }
                    RuntimeValue::Builtin(Builtin::GetElementById) => {
                        let id = args.first().map(|a| a.to_string()).unwrap_or_default();
                        let doc = self
                            .document
                            .clone()
                            .ok_or_else(|| JsError::ReferenceError("document is not defined".into()))?;
                        Ok(match DomNode::get_element_by_id(&doc, &id) {
                            Some(n) => RuntimeValue::Object(JsObject::Element(n)),
                            None => RuntimeValue::Null, // 見つからなければ null(仕様どおり)
                        })
                    }
                    other => Err(JsError::TypeError(format!("{other} is not a function"))),
                }
            }
        }

        /// + と -。+ はどちらかが文字列なら連結、そうでなければ数の足し算(ECMAScript の ApplyStringOrNumericBinaryOperator の縮小版)
        fn binary(op: char, l: RuntimeValue, r: RuntimeValue) -> Result<RuntimeValue, JsError> {
            use self::RuntimeValue::*;
            Ok(match (op, l, r) {
                ('+', Number(a), Number(b)) => Number(a + b),
                ('+', Str(a), b) => Str(format!("{a}{b}")),
                ('+', a, Str(b)) => Str(format!("{a}{b}")),
                ('-', Number(a), Number(b)) => Number(a - b),
                ('+', Undefined, Number(_)) | ('+', Number(_), Undefined) => Number(f64::NAN),
                (op, a, b) => return Err(JsError::TypeError(format!("{a} {op} {b} は計算できません"))),
            })
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::html::parser::HtmlParser;

            fn run(src: &str) -> RuntimeValue {
                JsRuntime::new(None).run(src).unwrap()
            }

            #[test]
            fn arithmetic() {
                assert_eq!(run("1 + 2"), RuntimeValue::Number(3.0));
                assert_eq!(run("10 - 2 - 3"), RuntimeValue::Number(5.0));
                assert_eq!(run("'a' + 1"), RuntimeValue::Str("a1".into()));
            }

            #[test]
            fn variables() {
                assert_eq!(run("var x = 1; var y = x + 2; y"), RuntimeValue::Number(3.0));
                assert_eq!(run("var x = 1; x = x + 1; x"), RuntimeValue::Number(2.0));
                assert!(matches!(
                    JsRuntime::new(None).run("nope"),
                    Err(JsError::ReferenceError(_))
                ));
            }

            #[test]
            fn functions_and_scope() {
                assert_eq!(run("function add(a, b) { return a + b; } add(1, 2)"), RuntimeValue::Number(3.0));
                // ローカル変数は外に漏れない
                assert!(matches!(
                    JsRuntime::new(None).run("function f() { var inner = 1; return inner; } f(); inner"),
                    Err(JsError::ReferenceError(_))
                ));
                // 外側の変数は読める(スコープチェーン)
                assert_eq!(run("var base = 10; function f(a) { return base + a; } f(5)"), RuntimeValue::Number(15.0));
                // 巻き上げ: 宣言より前に呼べる
                assert_eq!(run("g(); function g() { return 7; }"), RuntimeValue::Number(7.0));
            }

            #[test]
            fn console_log_collects_output() {
                let mut rt = JsRuntime::new(None);
                rt.run("console.log('a', 1 + 1); console.log('b')").unwrap();
                assert_eq!(rt.console_output, vec!["a 2", "b"]);
            }

            #[test]
            fn browser_api_reads_and_writes_dom() {
                let doc = HtmlParser::parse("<p id=\"msg\">before</p>");
                let mut rt = JsRuntime::new(Some(doc.clone()));
                assert_eq!(
                    rt.run("document.getElementById('msg').textContent").unwrap(),
                    RuntimeValue::Str("before".into())
                );
                rt.run("var el = document.getElementById('msg'); el.textContent = 'after ' + 1").unwrap();
                assert_eq!(DomNode::get_element_by_id(&doc, "msg").unwrap().text_content(), "after 1");
                assert!(rt.dom_changed);
                assert_eq!(rt.run("document.getElementById('none')").unwrap(), RuntimeValue::Null);
                assert!(matches!(
                    rt.run("document.getElementById('none').textContent"),
                    Err(JsError::TypeError(_))
                ));
            }
        }
    }
}

mod html {
    pub mod token {
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
    }
    pub mod tokenizer {
        //! HTML の字句解析。文字の列を「状態機械」で読み、トークンの列にする。
        //!
        //! HTML Standard 13.2.5 Tokenization:
        //! 「実装は、次の状態機械を使って HTML をトークン化するかのように振る舞わなければならない。
        //!   状態機械は data state から始まる。ほとんどの状態は 1 文字を消費し、
        //!   新しい状態へ切り替えて今の文字を再消費するか、新しい状態へ切り替えて次の文字を消費するか、
        //!   同じ状態にとどまって次の文字を消費する。」
        //!
        //! 仕様の状態は 84 個(13.2.5.1〜13.2.5.84)。この実装はそのうち 17 個 + 読み捨て用 1 個を持つ。
        //! 状態の名前と遷移の条件は仕様と同じにしてあるので、仕様の該当節と見比べながら読める。
        //!
        //! 持っていないもの: 文字参照(&amp; など)、コメント、DOCTYPE、RCDATA / RAWTEXT の各状態、
        //! script data の escape 系の状態。`<style>` と `<title>` の中身は script と同じ扱いで文字として読む。

        use std::collections::VecDeque;

        use crate::html::token::{Attribute, HtmlToken};

        /// 状態。名前は HTML Standard 13.2.5 の節の名前に合わせてある。
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum State {
            /// 13.2.5.1 タグの外側
            Data,
            /// 13.2.5.6 "<" を読んだ直後
            TagOpen,
            /// 13.2.5.7 "</" を読んだ直後
            EndTagOpen,
            /// 13.2.5.8 タグ名の途中
            TagName,
            /// 13.2.5.32 タグ名のあと、属性名の前
            BeforeAttributeName,
            /// 13.2.5.33 属性名の途中
            AttributeName,
            /// 13.2.5.34 属性名のあと("=" か次の属性か ">" を待つ)
            AfterAttributeName,
            /// 13.2.5.35 "=" のあと、値の前
            BeforeAttributeValue,
            /// 13.2.5.36 "..." の中
            AttributeValueDoubleQuoted,
            /// 13.2.5.37 '...' の中
            AttributeValueSingleQuoted,
            /// 13.2.5.38 引用符なしの値の中
            AttributeValueUnquoted,
            /// 13.2.5.39 閉じ引用符のあと
            AfterAttributeValueQuoted,
            /// 13.2.5.40 "/" のあと(">" が来れば自己終了タグ)
            SelfClosingStartTag,
            /// 13.2.5.4 `<script>` の中身。"<" が来てもタグにしない
            ScriptData,
            /// 13.2.5.15 script の中で "<" を読んだ直後
            ScriptDataLessThanSign,
            /// 13.2.5.16 script の中で "</" を読んだ直後
            ScriptDataEndTagOpen,
            /// 13.2.5.17 script の中の終了タグ名らしきものの途中
            ScriptDataEndTagName,
            /// 13.2.5.41 `<!...>` を ">" まで読み捨てる(DOCTYPE・コメントの代用)
            BogusComment,
        }

        /// 中身を「文字として」読む要素。本物は script data / RAWTEXT / RCDATA の 3 種類の状態を使い分けるが、
        /// 終了タグの見つけ方(「適切な終了タグ」が来るまで文字)は同じなので、この実装は 1 つの状態で扱う。
        const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style", "title", "textarea"];

        pub struct HtmlTokenizer {
            chars: Vec<char>,
            pos: usize,
            state: State,
            /// true なら次の consume() は直前の文字をもう一度返す(仕様の "reconsume")
            reconsume: bool,
            /// 作りかけのタグ
            current: Option<HtmlToken>,
            /// 最後に出した開始タグの名前。script data の中で「適切な終了タグ」かどうかを決めるのに使う
            last_start_tag: String,
            /// script data の中で "</" に続けて読んだ文字。終了タグでなかったら文字として流し直す
            temp: String,
            /// 1 回の遷移で 2 つ以上のトークンを出すときの待ち行列
            pending: VecDeque<HtmlToken>,
            finished: bool,
        }

        impl HtmlTokenizer {
            pub fn new(html: &str) -> Self {
                HtmlTokenizer {
                    chars: html.chars().collect(),
                    pos: 0,
                    state: State::Data,
                    reconsume: false,
                    current: None,
                    last_start_tag: String::new(),
                    temp: String::new(),
                    pending: VecDeque::new(),
                    finished: false,
                }
            }

            /// 今の状態(学習用に外から覗けるようにしてある)
            pub fn state(&self) -> State {
                self.state
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

            /// 「state に切り替えて、今の文字を再消費する」
            fn reconsume_in(&mut self, state: State) {
                self.reconsume = true;
                self.state = state;
            }

            fn new_start_tag(&mut self) {
                self.current = Some(HtmlToken::StartTag {
                    tag: String::new(),
                    self_closing: false,
                    attributes: Vec::new(),
                });
            }

            fn new_end_tag(&mut self) {
                self.current = Some(HtmlToken::EndTag { tag: String::new() });
            }

            fn append_tag_name(&mut self, c: char) {
                match &mut self.current {
                    Some(HtmlToken::StartTag { tag, .. }) | Some(HtmlToken::EndTag { tag }) => tag.push(c),
                    _ => {}
                }
            }

            fn new_attribute(&mut self) {
                if let Some(HtmlToken::StartTag { attributes, .. }) = &mut self.current {
                    attributes.push(Attribute::new("", ""));
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

            fn set_self_closing(&mut self) {
                if let Some(HtmlToken::StartTag { self_closing, .. }) = &mut self.current {
                    *self_closing = true;
                }
            }

            /// 作りかけのタグを完成させて返す。次の状態は原則 Data。
            /// ただし開始タグが script などなら、中身を文字として読む ScriptData に入る
            /// (本物では構文解析器がこの切り替えを指示する。ここでは字句解析器が自分で切り替える)。
            fn emit_current_tag(&mut self) -> HtmlToken {
                let token = self.current.take().expect("タグを作っている最中のはず");
                self.state = State::Data;
                if let HtmlToken::StartTag { tag, .. } = &token {
                    self.last_start_tag = tag.clone();
                    if RAW_TEXT_ELEMENTS.contains(&tag.as_str()) {
                        self.state = State::ScriptData;
                    }
                }
                token
            }

            /// 作りかけの終了タグが、最後の開始タグと同じ名前か(仕様の "appropriate end tag token")
            fn is_appropriate_end_tag(&self) -> bool {
                matches!(&self.current, Some(HtmlToken::EndTag { tag }) if *tag == self.last_start_tag)
            }
        }

        fn is_whitespace(c: char) -> bool {
            matches!(c, '\t' | '\n' | '\x0C' | ' ')
        }

        impl Iterator for HtmlTokenizer {
            type Item = HtmlToken;

            fn next(&mut self) -> Option<HtmlToken> {
                if let Some(t) = self.pending.pop_front() {
                    return Some(t);
                }
                if self.finished {
                    return None;
                }
                loop {
                    let c = match self.consume() {
                        Some(c) => c,
                        None => {
                            // どの状態で終わっても EOF トークンを 1 回出して終わる
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
                            '!' => self.state = State::BogusComment,
                            '/' => self.state = State::EndTagOpen,
                            c if c.is_ascii_alphabetic() => {
                                self.new_start_tag();
                                self.reconsume_in(State::TagName);
                            }
                            _ => {
                                // "<" の次がタグ名でなければ、"<" はただの文字
                                self.reconsume_in(State::Data);
                                return Some(HtmlToken::Char('<'));
                            }
                        },

                        State::EndTagOpen => match c {
                            c if c.is_ascii_alphabetic() => {
                                self.new_end_tag();
                                self.reconsume_in(State::TagName);
                            }
                            '>' => self.state = State::Data, // "</>" は読み捨て
                            _ => self.reconsume_in(State::BogusComment),
                        },

                        State::TagName => match c {
                            c if is_whitespace(c) => self.state = State::BeforeAttributeName,
                            '/' => self.state = State::SelfClosingStartTag,
                            '>' => return Some(self.emit_current_tag()),
                            _ => self.append_tag_name(c.to_ascii_lowercase()),
                        },

                        State::BeforeAttributeName => match c {
                            c if is_whitespace(c) => {}
                            '/' | '>' => self.reconsume_in(State::AfterAttributeName),
                            _ => {
                                self.new_attribute();
                                self.reconsume_in(State::AttributeName);
                            }
                        },

                        State::AttributeName => match c {
                            c if is_whitespace(c) || c == '/' || c == '>' => self.reconsume_in(State::AfterAttributeName),
                            '=' => self.state = State::BeforeAttributeValue,
                            _ => self.append_attribute_name(c.to_ascii_lowercase()),
                        },

                        State::AfterAttributeName => match c {
                            c if is_whitespace(c) => {}
                            '/' => self.state = State::SelfClosingStartTag,
                            '=' => self.state = State::BeforeAttributeValue,
                            '>' => return Some(self.emit_current_tag()),
                            _ => {
                                // 値のない属性の次の属性(例: <input disabled required>)
                                self.new_attribute();
                                self.reconsume_in(State::AttributeName);
                            }
                        },

                        State::BeforeAttributeValue => match c {
                            c if is_whitespace(c) => {}
                            '"' => self.state = State::AttributeValueDoubleQuoted,
                            '\'' => self.state = State::AttributeValueSingleQuoted,
                            '>' => return Some(self.emit_current_tag()), // 値が無い(parse error だがタグは出す)
                            _ => self.reconsume_in(State::AttributeValueUnquoted),
                        },

                        State::AttributeValueDoubleQuoted => match c {
                            '"' => self.state = State::AfterAttributeValueQuoted,
                            _ => self.append_attribute_value(c),
                        },

                        State::AttributeValueSingleQuoted => match c {
                            '\'' => self.state = State::AfterAttributeValueQuoted,
                            _ => self.append_attribute_value(c),
                        },

                        State::AttributeValueUnquoted => match c {
                            c if is_whitespace(c) => self.state = State::BeforeAttributeName,
                            '>' => return Some(self.emit_current_tag()),
                            _ => self.append_attribute_value(c),
                        },

                        State::AfterAttributeValueQuoted => match c {
                            c if is_whitespace(c) => self.state = State::BeforeAttributeName,
                            '/' => self.state = State::SelfClosingStartTag,
                            '>' => return Some(self.emit_current_tag()),
                            _ => self.reconsume_in(State::BeforeAttributeName), // 引用符の直後に空白が無い(parse error)
                        },

                        State::SelfClosingStartTag => match c {
                            '>' => {
                                self.set_self_closing();
                                return Some(self.emit_current_tag());
                            }
                            _ => self.reconsume_in(State::BeforeAttributeName),
                        },

                        State::ScriptData => match c {
                            '<' => self.state = State::ScriptDataLessThanSign,
                            _ => return Some(HtmlToken::Char(c)),
                        },

                        State::ScriptDataLessThanSign => match c {
                            '/' => {
                                self.temp.clear();
                                self.state = State::ScriptDataEndTagOpen;
                            }
                            _ => {
                                self.reconsume_in(State::ScriptData);
                                return Some(HtmlToken::Char('<'));
                            }
                        },

                        State::ScriptDataEndTagOpen => match c {
                            c if c.is_ascii_alphabetic() => {
                                self.new_end_tag();
                                self.reconsume_in(State::ScriptDataEndTagName);
                            }
                            _ => {
                                self.reconsume_in(State::ScriptData);
                                self.pending.push_back(HtmlToken::Char('/'));
                                return Some(HtmlToken::Char('<'));
                            }
                        },

                        State::ScriptDataEndTagName => {
                            let appropriate = self.is_appropriate_end_tag();
                            match c {
                                c if is_whitespace(c) && appropriate => self.state = State::BeforeAttributeName,
                                '/' if appropriate => self.state = State::SelfClosingStartTag,
                                '>' if appropriate => return Some(self.emit_current_tag()),
                                c if c.is_ascii_alphabetic() => {
                                    self.append_tag_name(c.to_ascii_lowercase());
                                    self.temp.push(c);
                                }
                                _ => {
                                    // 終了タグではなかった(例: script の中の "</scr" + "ipt>" 以外の文字列)。
                                    // "</" と読んだ文字をそのまま文字として流し、script data に戻る
                                    self.current = None;
                                    self.pending.push_back(HtmlToken::Char('/'));
                                    for t in self.temp.chars() {
                                        self.pending.push_back(HtmlToken::Char(t));
                                    }
                                    self.reconsume_in(State::ScriptData);
                                    return Some(HtmlToken::Char('<'));
                                }
                            }
                        }

                        State::BogusComment => match c {
                            '>' => self.state = State::Data,
                            _ => {}
                        },
                    }
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            fn tokens(html: &str) -> Vec<HtmlToken> {
                HtmlTokenizer::new(html).collect()
            }

            fn start(tag: &str, attrs: &[(&str, &str)]) -> HtmlToken {
                HtmlToken::StartTag {
                    tag: tag.to_string(),
                    self_closing: false,
                    attributes: attrs.iter().map(|(n, v)| Attribute::new(n, v)).collect(),
                }
            }

            fn end(tag: &str) -> HtmlToken {
                HtmlToken::EndTag { tag: tag.to_string() }
            }

            fn chars(s: &str) -> Vec<HtmlToken> {
                s.chars().map(HtmlToken::Char).collect()
            }

            #[test]
            fn empty_input_gives_only_eof() {
                assert_eq!(tokens(""), vec![HtmlToken::Eof]);
            }

            #[test]
            fn start_and_end_tag() {
                let mut expected = vec![start("p", &[])];
                expected.extend(chars("hi"));
                expected.push(end("p"));
                expected.push(HtmlToken::Eof);
                assert_eq!(tokens("<p>hi</p>"), expected);
            }

            #[test]
            fn tag_names_are_lowercased() {
                assert_eq!(tokens("<DIV></Div>"), vec![start("div", &[]), end("div"), HtmlToken::Eof]);
            }

            #[test]
            fn attributes_in_three_quote_styles() {
                let t = tokens(r#"<a href="/x" title='t' target=_blank>"#);
                assert_eq!(
                    t[0],
                    start("a", &[("href", "/x"), ("title", "t"), ("target", "_blank")])
                );
            }

            #[test]
            fn attribute_without_value_and_self_closing() {
                let t = tokens("<input disabled required/>");
                assert_eq!(
                    t[0],
                    HtmlToken::StartTag {
                        tag: "input".into(),
                        self_closing: true,
                        attributes: vec![Attribute::new("disabled", ""), Attribute::new("required", "")],
                    }
                );
            }

            #[test]
            fn lone_less_than_is_a_character() {
                let t = tokens("a < b");
                assert_eq!(t, {
                    let mut v = chars("a < b");
                    v.push(HtmlToken::Eof);
                    v
                });
            }

            #[test]
            fn script_content_is_not_parsed_as_tags() {
                let t = tokens("<script>if (a < b) { x = '</div>'; }</script>");
                let body: String = t[1..t.len() - 2]
                    .iter()
                    .map(|t| match t {
                        HtmlToken::Char(c) => *c,
                        other => panic!("script の中にタグが出た: {other:?}"),
                    })
                    .collect();
                assert_eq!(body, "if (a < b) { x = '</div>'; }");
                assert_eq!(t[t.len() - 2], end("script"));
            }

            #[test]
            fn doctype_and_comment_are_skipped() {
                assert_eq!(
                    tokens("<!DOCTYPE html><!-- c --><p>"),
                    vec![start("p", &[]), HtmlToken::Eof]
                );
            }
        }
    }
    pub mod dom {
        //! DOM ツリー。構文解析の出力で、CSS・レイアウト・JavaScript が読み書きする土台。
        //!
        //! DOM Standard 4.2 Node tree: ノードは parent(親)と children(子の順序付き集合)を持つ。
        //! この実装は Document / Element / Text の 3 種類だけ(Comment・DocumentType は持たない)。
        //!
        //! Rust でツリーを持つときの定番:
        //! - 子は `Rc<Node>`(所有者が複数になりうる: 親と、JavaScript から掴んだ参照)
        //! - 親は `Weak<Node>`(親→子→親 の循環参照でメモリが解放されなくなるのを避ける)
        //! - 子のリストは `RefCell` で包み、`&self` のまま付け外しできるようにする(内部可変性)

        use std::cell::RefCell;
        use std::fmt::Write as _;
        use std::rc::{Rc, Weak};

        use crate::html::token::Attribute;

        pub type NodeRef = Rc<Node>;

        /// 要素ノードの中身。
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Element {
            pub tag: String,
            pub attributes: Vec<Attribute>,
        }

        impl Element {
            pub fn attribute(&self, name: &str) -> Option<&str> {
                self.attributes
                    .iter()
                    .find(|a| a.name == name)
                    .map(|a| a.value.as_str())
            }

            pub fn id(&self) -> Option<&str> {
                self.attribute("id")
            }

            /// class 属性を空白で区切ったもの
            pub fn classes(&self) -> Vec<&str> {
                self.attribute("class")
                    .map(|c| c.split_whitespace().collect())
                    .unwrap_or_default()
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum NodeKind {
            Document,
            Element(Element),
            Text(String),
        }

        #[derive(Debug)]
        pub struct Node {
            pub kind: NodeKind,
            parent: RefCell<Weak<Node>>,
            children: RefCell<Vec<NodeRef>>,
        }

        impl Node {
            pub fn new(kind: NodeKind) -> NodeRef {
                Rc::new(Node {
                    kind,
                    parent: RefCell::new(Weak::new()),
                    children: RefCell::new(Vec::new()),
                })
            }

            pub fn document() -> NodeRef {
                Node::new(NodeKind::Document)
            }

            pub fn element(tag: &str, attributes: Vec<Attribute>) -> NodeRef {
                Node::new(NodeKind::Element(Element {
                    tag: tag.to_string(),
                    attributes,
                }))
            }

            pub fn text(data: &str) -> NodeRef {
                Node::new(NodeKind::Text(data.to_string()))
            }

            /// 末尾に子を追加する(DOM の appendChild)
            pub fn append_child(parent: &NodeRef, child: NodeRef) {
                *child.parent.borrow_mut() = Rc::downgrade(parent);
                parent.children.borrow_mut().push(child);
            }

            /// 子を全部外す
            pub fn remove_children(parent: &NodeRef) {
                for c in parent.children.borrow().iter() {
                    *c.parent.borrow_mut() = Weak::new();
                }
                parent.children.borrow_mut().clear();
            }

            pub fn parent(&self) -> Option<NodeRef> {
                self.parent.borrow().upgrade()
            }

            /// 子のリストの複製(Rc なので中身は共有される)
            pub fn children(&self) -> Vec<NodeRef> {
                self.children.borrow().clone()
            }

            pub fn first_child(&self) -> Option<NodeRef> {
                self.children.borrow().first().cloned()
            }

            pub fn last_child(&self) -> Option<NodeRef> {
                self.children.borrow().last().cloned()
            }

            pub fn as_element(&self) -> Option<&Element> {
                match &self.kind {
                    NodeKind::Element(e) => Some(e),
                    _ => None,
                }
            }

            pub fn tag(&self) -> Option<&str> {
                self.as_element().map(|e| e.tag.as_str())
            }

            pub fn is_element(&self, tag: &str) -> bool {
                self.tag() == Some(tag)
            }

            pub fn text_data(&self) -> Option<&str> {
                match &self.kind {
                    NodeKind::Text(t) => Some(t.as_str()),
                    _ => None,
                }
            }

            /// 子孫のテキストをつないだもの(DOM の textContent の getter)
            pub fn text_content(&self) -> String {
                match &self.kind {
                    NodeKind::Text(t) => t.clone(),
                    NodeKind::Document => String::new(),
                    NodeKind::Element(_) => self.children.borrow().iter().map(|c| c.text_content()).collect(),
                }
            }

            /// textContent の setter。DOM Standard の "string replace all":
            /// 子を全部外し、空でなければ新しい Text ノードを 1 つ付ける
            pub fn set_text_content(node: &NodeRef, value: &str) {
                Node::remove_children(node);
                if !value.is_empty() {
                    Node::append_child(node, Node::text(value));
                }
            }

            /// DOM Standard: 「子孫のうち tree order で最初の、ID が elementId である要素。無ければ null」
            pub fn get_element_by_id(root: &NodeRef, id: &str) -> Option<NodeRef> {
                for child in root.children.borrow().iter() {
                    if child.as_element().and_then(|e| e.id()) == Some(id) {
                        return Some(child.clone());
                    }
                    if let Some(found) = Node::get_element_by_id(child, id) {
                        return Some(found);
                    }
                }
                None
            }

            /// タグ名で子孫を集める(tree order)
            pub fn find_all(root: &NodeRef, tag: &str) -> Vec<NodeRef> {
                let mut out = Vec::new();
                Node::collect(root, tag, &mut out);
                out
            }

            fn collect(node: &NodeRef, tag: &str, out: &mut Vec<NodeRef>) {
                for child in node.children.borrow().iter() {
                    if child.is_element(tag) {
                        out.push(child.clone());
                    }
                    Node::collect(child, tag, out);
                }
            }

            /// ツリーを字下げした文字列にする(学習用・デバッグ用)
            pub fn dump(node: &NodeRef) -> String {
                let mut out = String::new();
                Node::dump_into(node, 0, &mut out);
                out
            }

            fn dump_into(node: &NodeRef, depth: usize, out: &mut String) {
                let indent = "  ".repeat(depth);
                match &node.kind {
                    NodeKind::Document => writeln!(out, "{indent}#document").unwrap(),
                    NodeKind::Element(e) => {
                        let attrs: String = e
                            .attributes
                            .iter()
                            .map(|a| format!(" {}=\"{}\"", a.name, a.value))
                            .collect();
                        writeln!(out, "{indent}<{}{}>", e.tag, attrs).unwrap();
                    }
                    NodeKind::Text(t) => {
                        if t.trim().is_empty() {
                            writeln!(out, "{indent}#text (空白のみ)").unwrap();
                        } else {
                            writeln!(out, "{indent}#text {:?}", t).unwrap();
                        }
                    }
                }
                for c in node.children.borrow().iter() {
                    Node::dump_into(c, depth + 1, out);
                }
            }
        }

        /// テストで「同じ形のツリーか」を比べるための等値。親は比べない(子から辿れるので)
        impl PartialEq for Node {
            fn eq(&self, other: &Self) -> bool {
                self.kind == other.kind && *self.children.borrow() == *other.children.borrow()
            }
        }

        impl Eq for Node {}

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn append_sets_parent_and_children() {
                let doc = Node::document();
                let html = Node::element("html", vec![]);
                let body = Node::element("body", vec![]);
                Node::append_child(&doc, html.clone());
                Node::append_child(&html, body.clone());
                assert_eq!(body.parent().unwrap().tag(), Some("html"));
                assert_eq!(html.parent().unwrap().kind, NodeKind::Document);
                assert!(doc.parent().is_none());
                assert_eq!(doc.children().len(), 1);
                assert_eq!(Rc::strong_count(&body), 2); // 変数 body と、html の children
            }

            #[test]
            fn text_content_joins_descendants() {
                let p = Node::element("p", vec![]);
                Node::append_child(&p, Node::text("Hello, "));
                let b = Node::element("b", vec![]);
                Node::append_child(&b, Node::text("world"));
                Node::append_child(&p, b);
                assert_eq!(p.text_content(), "Hello, world");

                Node::set_text_content(&p, "replaced");
                assert_eq!(p.children().len(), 1);
                assert_eq!(p.text_content(), "replaced");
                Node::set_text_content(&p, "");
                assert_eq!(p.children().len(), 0);
            }

            #[test]
            fn get_element_by_id_is_tree_order() {
                let root = Node::element("div", vec![]);
                let a = Node::element("p", vec![Attribute::new("id", "x")]);
                let inner = Node::element("span", vec![Attribute::new("id", "y")]);
                Node::append_child(&a, inner);
                let b = Node::element("p", vec![Attribute::new("id", "y")]);
                Node::append_child(&root, a);
                Node::append_child(&root, b);
                let found = Node::get_element_by_id(&root, "y").unwrap();
                assert_eq!(found.tag(), Some("span")); // 深い方が先(tree order)
                assert!(Node::get_element_by_id(&root, "nope").is_none());
            }

            #[test]
            fn dropping_the_tree_frees_nodes() {
                let doc = Node::document();
                let child = Node::element("p", vec![]);
                let weak = Rc::downgrade(&child);
                Node::append_child(&doc, child);
                assert!(weak.upgrade().is_some());
                drop(doc); // 親が消えれば子も消える(親→子が Rc、子→親が Weak だから循環しない)
                assert!(weak.upgrade().is_none());
            }
        }
    }
    pub mod parser {
        //! HTML の構文解析。トークンの列から DOM ツリーを組み立てる(HTML Standard 13.2.6 Tree construction)。
        //!
        //! 仕様: 「構文解析器の入力は字句解析器が出すトークンの列。出力はその Document の DOM ツリーを
        //! 動的に変更・拡張したもの。」
        //!
        //! 2 つの状態で動く:
        //! - insertion mode(挿入モード): 「構文解析の主要な動作を制御する状態変数」。
        //!   仕様には 23 個あり、この実装は 9 個(initial → before html → before head → in head → after head →
        //!   in body / text → after body → after after body)。
        //! - stack of open elements(開いている要素のスタック): 「最初は空。最後に追加したものが一番下(= 今の要素)」。
        //!
        //! 仕様のどの節の処理を簡略化したかは各モードのコメントに書いた。

        use crate::html::dom::{Node, NodeRef};
        use crate::html::token::{Attribute, HtmlToken};
        use crate::html::tokenizer::HtmlTokenizer;

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum InsertionMode {
            Initial,
            BeforeHtml,
            BeforeHead,
            InHead,
            AfterHead,
            InBody,
            /// script / style / title の中身を文字として集めるモード
            Text,
            AfterBody,
            AfterAfterBody,
        }

        /// 終了タグを持たない要素(HTML Standard 13.1.2 の void elements)
        const VOID_ELEMENTS: &[&str] = &[
            "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr",
        ];

        /// head の中で中身を文字として読む要素
        const HEAD_TEXT_ELEMENTS: &[&str] = &["script", "style", "title"];

        /// 開始タグが来たとき、開いている <p> を先に閉じる要素(仕様 "in body" の address, article, ... の規則の一部)
        const CLOSES_P: &[&str] = &[
            "address", "article", "aside", "blockquote", "div", "dl", "fieldset", "footer", "form", "h1", "h2", "h3",
            "h4", "h5", "h6", "header", "hr", "main", "nav", "ol", "p", "pre", "section", "table", "ul",
        ];

        pub struct HtmlParser {
            tokenizer: HtmlTokenizer,
            mode: InsertionMode,
            /// Text モードから戻る先
            original_mode: InsertionMode,
            document: NodeRef,
            open_elements: Vec<NodeRef>,
            /// まだノードにしていない連続した文字
            text_buffer: String,
            /// モードの遷移の記録(学習用)
            trace: Vec<String>,
        }

        impl HtmlParser {
            pub fn new(html: &str) -> Self {
                HtmlParser {
                    tokenizer: HtmlTokenizer::new(html),
                    mode: InsertionMode::Initial,
                    original_mode: InsertionMode::Initial,
                    document: Node::document(),
                    open_elements: Vec::new(),
                    text_buffer: String::new(),
                    trace: Vec::new(),
                }
            }

            /// HTML 文字列から Document を作る
            pub fn parse(html: &str) -> NodeRef {
                HtmlParser::new(html).run().0
            }

            /// Document と、挿入モードの遷移の記録を返す
            pub fn run(mut self) -> (NodeRef, Vec<String>) {
                self.construct_tree();
                (self.document, self.trace)
            }

            fn construct_tree(&mut self) {
                let mut token = self.tokenizer.next();
                while let Some(t) = token.take() {
                    if !matches!(t, HtmlToken::Char(_)) {
                        self.flush_text();
                    }
                    let reprocess = self.step(&t);
                    if matches!(t, HtmlToken::Eof) && !reprocess {
                        break;
                    }
                    token = if reprocess { Some(t) } else { self.tokenizer.next() };
                }
                self.flush_text();
            }

            /// 1 トークンを今のモードで処理する。true を返したら同じトークンを新しいモードでもう一度処理する
            /// (仕様の "reprocess the token")
            fn step(&mut self, token: &HtmlToken) -> bool {
                use self::HtmlToken::*;
                use self::InsertionMode::*;

                match self.mode {
                    // 13.2.6.4.1 "initial": 本物は DOCTYPE を見る。この実装は DOCTYPE を字句解析で捨てているので
                    // 空白を読み飛ばし、それ以外が来たら before html へ
                    Initial => match token {
                        Char(c) if c.is_ascii_whitespace() => false,
                        _ => {
                            self.switch(BeforeHtml, token);
                            true
                        }
                    },

                    // 13.2.6.4.2 "before html": <html> があればそれを、無ければ暗黙の <html> を作る
                    BeforeHtml => match token {
                        Char(c) if c.is_ascii_whitespace() => false,
                        StartTag { tag, attributes, .. } if tag == "html" => {
                            self.insert_element("html", attributes.clone());
                            self.switch(BeforeHead, token);
                            false
                        }
                        EndTag { tag } if !["head", "body", "html", "br"].contains(&tag.as_str()) => false,
                        _ => {
                            self.insert_element("html", vec![]);
                            self.switch(BeforeHead, token);
                            true
                        }
                    },

                    // 13.2.6.4.3 "before head"
                    BeforeHead => match token {
                        Char(c) if c.is_ascii_whitespace() => false,
                        StartTag { tag, attributes, .. } if tag == "head" => {
                            self.insert_element("head", attributes.clone());
                            self.switch(InHead, token);
                            false
                        }
                        EndTag { tag } if !["head", "body", "html", "br"].contains(&tag.as_str()) => false,
                        _ => {
                            self.insert_element("head", vec![]);
                            self.switch(InHead, token);
                            true
                        }
                    },

                    // 13.2.6.4.4 "in head": style / script / title は中身を文字として集める。meta / link は子を持たない
                    InHead => match token {
                        Char(c) if c.is_ascii_whitespace() => {
                            self.text_buffer.push(*c);
                            false
                        }
                        StartTag { tag, attributes, .. } if HEAD_TEXT_ELEMENTS.contains(&tag.as_str()) => {
                            self.insert_element(tag, attributes.clone());
                            self.original_mode = InHead;
                            self.switch(Text, token);
                            false
                        }
                        StartTag { tag, attributes, .. } if VOID_ELEMENTS.contains(&tag.as_str()) => {
                            self.insert_element(tag, attributes.clone());
                            self.open_elements.pop();
                            false
                        }
                        EndTag { tag } if tag == "head" => {
                            self.open_elements.pop();
                            self.switch(AfterHead, token);
                            false
                        }
                        EndTag { tag } if !["body", "html", "br"].contains(&tag.as_str()) => false,
                        _ => {
                            // head の中に書けないものが来た = head はここで終わり
                            self.open_elements.pop();
                            self.switch(AfterHead, token);
                            true
                        }
                    },

                    // 13.2.6.4.6 "after head": <body> があればそれを、無ければ暗黙の <body> を作る
                    AfterHead => match token {
                        Char(c) if c.is_ascii_whitespace() => {
                            self.text_buffer.push(*c);
                            false
                        }
                        StartTag { tag, attributes, .. } if tag == "body" => {
                            self.insert_element("body", attributes.clone());
                            self.switch(InBody, token);
                            false
                        }
                        StartTag { tag, attributes, .. } if HEAD_TEXT_ELEMENTS.contains(&tag.as_str()) => {
                            // 本物は head をスタックに戻して "in head" の規則で処理する。ここでは直接 head に入れる
                            if let Some(head) = self.find_head() {
                                let el = Node::element(tag, attributes.clone());
                                Node::append_child(&head, el.clone());
                                self.open_elements.push(el);
                                self.original_mode = AfterHead;
                                self.switch(Text, token);
                            }
                            false
                        }
                        EndTag { tag } if !["body", "html", "br"].contains(&tag.as_str()) => false,
                        _ => {
                            self.insert_element("body", vec![]);
                            self.switch(InBody, token);
                            true
                        }
                    },

                    // 13.2.6.4.7 "in body": 一番長い節。ここでは「要素を入れて積む」「終了タグで積んだ分を下ろす」が芯
                    InBody => match token {
                        Char(c) => {
                            self.text_buffer.push(*c);
                            false
                        }
                        StartTag { tag, attributes, .. } => {
                            match tag.as_str() {
                                "html" | "head" | "body" => return false, // 2 回目は無視(parse error)
                                _ => {}
                            }
                            // <p> が開いたまま <div> や <h1> が来たら、<p> はそこで閉じる(仕様 "close a p element")
                            if CLOSES_P.contains(&tag.as_str()) && self.has_in_scope("p") {
                                self.pop_until("p");
                            }
                            self.insert_element(tag, attributes.clone());
                            // void 要素は子を持たないので、置いた直後に下ろす。
                            // <div/> のように void 以外に付いた "/"(self_closing)は、仕様では無視される(parse error)。
                            // だから <div/>x の x は div の中に入る
                            if VOID_ELEMENTS.contains(&tag.as_str()) {
                                self.open_elements.pop();
                            }
                            false
                        }
                        EndTag { tag } => {
                            match tag.as_str() {
                                "body" => {
                                    if self.has_in_scope("body") {
                                        self.switch(AfterBody, token);
                                    }
                                    false
                                }
                                "html" => {
                                    if self.has_in_scope("body") {
                                        self.switch(AfterBody, token);
                                        return true;
                                    }
                                    false
                                }
                                "br" => {
                                    // </br> は <br> として扱う(仕様どおり)
                                    self.insert_element("br", vec![]);
                                    self.open_elements.pop();
                                    false
                                }
                                _ => {
                                    if self.has_in_scope(tag) {
                                        self.pop_until(tag);
                                    }
                                    // 対応する開始タグが無い終了タグは無視(parse error)
                                    false
                                }
                            }
                        }
                        Eof => false,
                    },

                    // 13.2.6.4.8 "text": 終了タグが来るまで文字を集める
                    Text => match token {
                        Char(c) => {
                            self.text_buffer.push(*c);
                            false
                        }
                        EndTag { .. } => {
                            self.open_elements.pop();
                            let back = self.original_mode;
                            self.switch(back, token);
                            false
                        }
                        _ => {
                            // EOF(閉じ忘れ): 要素を閉じて元のモードで EOF を処理し直す
                            self.open_elements.pop();
                            let back = self.original_mode;
                            self.switch(back, token);
                            true
                        }
                    },

                    // 13.2.6.4.19 "after body"
                    AfterBody => match token {
                        Char(c) if c.is_ascii_whitespace() => {
                            self.text_buffer.push(*c);
                            false
                        }
                        EndTag { tag } if tag == "html" => {
                            self.switch(AfterAfterBody, token);
                            false
                        }
                        Eof => false,
                        _ => {
                            // </body> の後に中身が来た(parse error): body に戻って続ける
                            self.switch(InBody, token);
                            true
                        }
                    },

                    // 13.2.6.4.22 "after after body"
                    AfterAfterBody => match token {
                        Char(c) if c.is_ascii_whitespace() => {
                            self.text_buffer.push(*c);
                            false
                        }
                        Eof => false,
                        _ => {
                            self.switch(InBody, token);
                            true
                        }
                    },
                }
            }

            fn switch(&mut self, to: InsertionMode, because_of: &HtmlToken) {
                self.trace
                    .push(format!("{:?} → {:?}  ({})", self.mode, to, describe(because_of)));
                self.mode = to;
            }

            /// 今の要素(スタックの一番下)。空なら document
            fn current_node(&self) -> NodeRef {
                self.open_elements
                    .last()
                    .cloned()
                    .unwrap_or_else(|| self.document.clone())
            }

            /// 要素を作って今の要素の子にし、スタックに積む(仕様 "insert an HTML element")
            fn insert_element(&mut self, tag: &str, attributes: Vec<Attribute>) -> NodeRef {
                let el = Node::element(tag, attributes);
                Node::append_child(&self.current_node(), el.clone());
                self.open_elements.push(el.clone());
                el
            }

            /// 溜めた文字を Text ノードにして今の要素に付ける(仕様 "insert a character" をまとめてやる)
            fn flush_text(&mut self) {
                if self.text_buffer.is_empty() {
                    return;
                }
                let text = std::mem::take(&mut self.text_buffer);
                let parent = self.current_node();
                // document 直下に文字は置けない(before html などで空白を無視しているので通常は来ない)
                if parent.as_element().is_some() {
                    Node::append_child(&parent, Node::text(&text));
                }
            }

            /// スタックにその要素があるか(仕様の "has an element in scope" を単純化したもの)
            fn has_in_scope(&self, tag: &str) -> bool {
                self.open_elements.iter().any(|n| n.is_element(tag))
            }

            /// その要素が下ろされるまでスタックを下ろす
            fn pop_until(&mut self, tag: &str) {
                while let Some(n) = self.open_elements.pop() {
                    if n.is_element(tag) {
                        break;
                    }
                }
            }

            fn find_head(&self) -> Option<NodeRef> {
                let html = self.document.first_child()?;
                html.children().into_iter().find(|n| n.is_element("head"))
            }
        }

        fn describe(token: &HtmlToken) -> String {
            match token {
                HtmlToken::StartTag { tag, .. } => format!("<{tag}>"),
                HtmlToken::EndTag { tag } => format!("</{tag}>"),
                HtmlToken::Char(c) => format!("文字 {c:?}"),
                HtmlToken::Eof => "EOF".to_string(),
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::html::dom::NodeKind;

            fn tags_of(node: &NodeRef) -> Vec<String> {
                node.children()
                    .iter()
                    .map(|c| match &c.kind {
                        NodeKind::Element(e) => e.tag.clone(),
                        NodeKind::Text(t) => format!("#text({})", t.trim()),
                        NodeKind::Document => "#document".into(),
                    })
                    .collect()
            }

            fn body_of(doc: &NodeRef) -> NodeRef {
                let html = doc.first_child().unwrap();
                html.children().into_iter().find(|n| n.is_element("body")).unwrap()
            }

            #[test]
            fn empty_input_still_makes_html_head_body() {
                let doc = HtmlParser::parse("");
                let html = doc.first_child().unwrap();
                assert_eq!(html.tag(), Some("html"));
                assert_eq!(tags_of(&html), vec!["head", "body"]);
            }

            #[test]
            fn text_only_goes_into_body() {
                let doc = HtmlParser::parse("hello");
                assert_eq!(tags_of(&body_of(&doc)), vec!["#text(hello)"]);
            }

            #[test]
            fn full_document() {
                let doc = HtmlParser::parse(
                    "<html><head><title>T</title></head><body><h1 id=\"t\">Hi</h1><p>a <a href=\"/x\">link</a></p></body></html>",
                );
                let html = doc.first_child().unwrap();
                assert_eq!(tags_of(&html), vec!["head", "body"]);
                let head = html.first_child().unwrap();
                assert_eq!(tags_of(&head), vec!["title"]);
                assert_eq!(head.first_child().unwrap().text_content(), "T");
                let body = body_of(&doc);
                assert_eq!(tags_of(&body), vec!["h1", "p"]);
                let p = body.last_child().unwrap();
                assert_eq!(tags_of(&p), vec!["#text(a)", "a"]);
                assert_eq!(Node::get_element_by_id(&doc, "t").unwrap().text_content(), "Hi");
            }

            #[test]
            fn missing_end_tags_are_repaired() {
                // <p> の閉じ忘れ: 次の <p> が来た時点で前の <p> は閉じる(兄弟になる。入れ子にならない)
                let doc = HtmlParser::parse("<p>one<p>two");
                assert_eq!(tags_of(&body_of(&doc)), vec!["p", "p"]);
                // <div> の中の <p> の閉じ忘れ: </div> で p も閉じる
                let doc = HtmlParser::parse("<div><p>x</div><span>y</span>");
                assert_eq!(tags_of(&body_of(&doc)), vec!["div", "span"]);
            }

            #[test]
            fn self_closing_slash_is_ignored_on_non_void_elements() {
                // <br/> は void なので子を持たない。<div/> の "/" は無視されて div は開いたまま(仕様どおり)
                let doc = HtmlParser::parse("<p>a<br/>b</p><div/>x");
                let body = body_of(&doc);
                assert_eq!(tags_of(&body), vec!["p", "div"]);
                assert_eq!(body.last_child().unwrap().text_content(), "x");
            }

            #[test]
            fn stray_end_tag_is_ignored() {
                let doc = HtmlParser::parse("<p>a</b>b</p>");
                let p = body_of(&doc).first_child().unwrap();
                assert_eq!(p.text_content(), "ab");
            }

            #[test]
            fn style_and_script_keep_raw_text() {
                let doc = HtmlParser::parse("<style>p > a { color: red }</style><script>if (1 < 2) {}</script><p>x</p>");
                let styles = Node::find_all(&doc, "style");
                assert_eq!(styles[0].text_content(), "p > a { color: red }");
                let scripts = Node::find_all(&doc, "script");
                assert_eq!(scripts[0].text_content(), "if (1 < 2) {}");
                assert_eq!(tags_of(&body_of(&doc)), vec!["p"]);
            }

            #[test]
            fn mode_trace_is_recorded() {
                let (_, trace) = HtmlParser::new("<p>x</p>").run();
                assert_eq!(trace[0], "Initial → BeforeHtml  (<p>)");
                assert!(trace.iter().any(|t| t.contains("AfterHead → InBody")));
            }
        }
    }
}

use js::runtime::JsRuntime;

fn show(src: &str) {
    println!("── {src}");
    let mut rt = JsRuntime::new(None);
    match rt.run(src) {
        Ok(v) => println!("   結果: {v}"),
        Err(e) => println!("   {e}"),
    }
    for line in &rt.console_output {
        println!("   [console] {line}");
    }
    println!();
}

fn main() {
    show("1 + 2");
    show("'a' + 1 + 2");
    show("var x = 10; var y = x - 3; y");
    show("var x = 1; x = x + 1; x");
    show("function add(a, b) { return a + b; } add(1, 2)");
    show("var base = 10; function f(a) { return base + a; } f(5)");
    show("function f() { var inner = 1; return inner; } f(); inner");
    show("g(); function g() { return 7; }");
    show("console.log('合計は', 1 + 2); console.log('二行目')");
    show("nope + 1");
    show("(1)(2)");
}
