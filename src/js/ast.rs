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
