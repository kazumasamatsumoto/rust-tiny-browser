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
