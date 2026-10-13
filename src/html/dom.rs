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
