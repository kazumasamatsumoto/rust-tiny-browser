// ステップ 12: トークンから抽象構文木(AST)を組み立てる
//
// このファイル 1 つで動きます:
//   実行:      rustc --edition 2021 step12_js_parser.rs && ./step12_js_parser
//   テスト:    rustc --edition 2021 --test step12_js_parser.rs -o step12_js_parser_test && ./step12_js_parser_test
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
}

use js::parser::JsParser;

fn show(src: &str) {
    println!("── {src}");
    match JsParser::parse(src) {
        Ok(program) => {
            for node in &program.body {
                print!("{}", node.dump());
            }
        }
        Err(e) => println!("   {e}"),
    }
    println!();
}

fn main() {
    show("1 + 2 - 3");
    show("var x = 1; x = x + 1;");
    show("function add(a, b) { return a + b; } add(1, 2)");
    show("document.getElementById('t').textContent = 'hi'");
    show("var = 1");
    show("f(");
}
