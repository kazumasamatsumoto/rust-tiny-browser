#!/usr/bin/env python3
"""教材(サイト)が import するコード集 lib/handson/rust-browser-code.ts を作る。

- MODULES[path]      : src/ のモジュール(テスト部分を除いたもの)。レッスンの「読む」用
- MODULE_TESTS[path] : そのモジュールのテスト部分だけ
- STEPS[id]          : steps/run/<id>.rs(テスト抜きの 1 ファイル版)。レッスンの「実行」用
- EXCERPTS[key]      : 関数単位の抜粋(レッスンで部分だけを見せる用)
"""
import json
import os
import re

ROOT = os.path.join(os.path.dirname(__file__), "..")
SRC = os.path.join(ROOT, "src")
STEPS = os.path.join(ROOT, "steps")

MODULE_PATHS = [
    "lib.rs", "main.rs", "url.rs", "http.rs",
    "html/token.rs", "html/tokenizer.rs", "html/dom.rs", "html/parser.rs",
    "css/token.rs", "css/tokenizer.rs", "css/cssom.rs", "css/parser.rs",
    "renderer/style.rs", "renderer/layout.rs", "renderer/paint.rs",
    "js/lexer.rs", "js/ast.rs", "js/parser.rs", "js/runtime.rs",
    "browser.rs",
]

# サイトの実行欄に載せるのは 50KB 以下のものだけ(大きいステップは配布物として置く)
RUN_STEPS = [
    "step01_url", "step02_http_message", "step03_http_fetch", "step04_html_tokenizer", "step05a_dom_nodes", "step05_dom",
    "step06_css_tokenizer", "step07_cssom", "step11_js_lexer", "step12_js_parser",
]
MINI_STEPS = ["mini01_state_machine", "mini02_attributes"]

# 抜粋: (キー, モジュール, 先頭行の目印)。目印の行から、対応する閉じ波括弧までを切り出す
EXCERPTS = [
    ("html_parser_step", "html/parser.rs", "    fn step(&mut self, token: &HtmlToken) -> bool {"),
    ("html_parser_in_body", "html/parser.rs", "            InBody => match token {"),
    ("html_parser_helpers", "html/parser.rs", "    fn switch(&mut self, to: InsertionMode, because_of: &HtmlToken) {"),
    ("style_compute", "renderer/style.rs", "pub fn compute_style(el: &Element, sheet: &StyleSheet, parent: Option<&ComputedStyle>) -> ComputedStyle {"),
    ("style_ua_defaults", "renderer/style.rs", "fn apply_user_agent_defaults(style: &mut ComputedStyle, tag: &str) {"),
    ("style_apply_declaration", "renderer/style.rs", "fn apply_declaration(style: &mut ComputedStyle, d: &Declaration) {"),
    ("cssom_selector_impl", "css/cssom.rs", "impl Selector {"),
    ("layout_build_tree", "renderer/layout.rs", "pub fn build_layout_tree(node: &NodeRef, sheet: &StyleSheet, parent_style: Option<&ComputedStyle>) -> Option<LayoutBox> {"),
    ("layout_block", "renderer/layout.rs", "fn layout_block(b: &mut LayoutBox, x: f32, y: f32, container_width: f32) {"),
    ("layout_flow", "renderer/layout.rs", "fn layout_flow(children: &mut [LayoutBox], cur: &mut Cursor, container_width: f32) {"),
    ("paint_fn", "renderer/paint.rs", "pub fn paint(root: &LayoutBox) -> Vec<DisplayItem> {"),
    ("paint_box", "renderer/paint.rs", "fn paint_box(b: &LayoutBox, items: &mut Vec<DisplayItem>) {"),
    ("paint_render_ascii", "renderer/paint.rs", "pub fn render_ascii(items: &[DisplayItem], width_px: f32) -> String {"),
    ("js_parser_additive", "js/parser.rs", "    fn parse_additive_expression(&mut self) -> Result<Node, ParseError> {"),
    ("js_parser_assignment", "js/parser.rs", "    fn parse_assignment_expression(&mut self) -> Result<Node, ParseError> {"),
    ("js_parser_lhs", "js/parser.rs", "    fn parse_left_hand_side_expression(&mut self) -> Result<Node, ParseError> {"),
    ("js_runtime_env", "js/runtime.rs", "impl Environment {"),
    ("js_runtime_call", "js/runtime.rs", "    fn call(&mut self, f: RuntimeValue, args: Vec<RuntimeValue>) -> Result<RuntimeValue, JsError> {"),
    ("js_runtime_exec_block", "js/runtime.rs", "    fn exec_block(&mut self, statements: &[Node], env: &EnvRef) -> Result<Completion, JsError> {"),
    ("js_runtime_member", "js/runtime.rs", "            Node::MemberExpression { object, property } => {"),
    ("js_runtime_assignment", "js/runtime.rs", "            Node::AssignmentExpression { target, value } => {"),
    ("browser_page_load", "browser.rs", "impl Page {"),
    ("tokenizer_script_states", "html/tokenizer.rs", "                State::ScriptData => match c {"),
    ("tokenizer_emit_current_tag", "html/tokenizer.rs", "    fn emit_current_tag(&mut self) -> HtmlToken {"),
    ("http_fetch", "http.rs", "pub fn fetch(url: &Url) -> Result<HttpResponse, HttpError> {"),
    ("http_parse", "http.rs", "    pub fn parse(raw: &str) -> Result<HttpResponse, HttpError> {"),
    ("http_build_request", "http.rs", "pub fn build_request(url: &Url) -> String {"),
    ("http_decode_chunked", "http.rs", "fn decode_chunked(body: &str) -> Result<String, HttpError> {"),
    ("url_parse", "url.rs", "pub fn parse(input: &str) -> Result<Url, UrlError> {"),
    ("dom_struct", "html/dom.rs", "#[derive(Debug)]\npub struct Node {"),
    ("dom_text_content", "html/dom.rs", "    pub fn text_content(&self) -> String {"),
    ("dom_get_element_by_id", "html/dom.rs", "    pub fn get_element_by_id(root: &NodeRef, id: &str) -> Option<NodeRef> {"),
]


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def split_tests(text):
    i = text.find("\n#[cfg(test)]")
    if i < 0:
        return text.rstrip("\n"), ""
    return text[:i].rstrip("\n"), text[i + 1 :].rstrip("\n")


def brace_block(text, start_marker):
    """start_marker の行から、対応する閉じ括弧の行までを返す。文字列・文字・コメントの中の括弧は数えない"""
    start = text.find(start_marker)
    if start < 0:
        raise SystemExit(f"目印が見つかりません: {start_marker!r}")
    i = text.find("{", start)
    depth = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c == "/" and text.startswith("//", i):
            i = text.find("\n", i)
            if i < 0:
                break
            continue
        if c == "/" and text.startswith("/*", i):
            i = text.find("*/", i) + 2
            continue
        if c == "r" and re.match(r'r#*"', text[i : i + 4]):
            m = re.match(r'r(#*)"', text[i:])
            hashes = m.group(1)
            end = text.find('"' + hashes, i + len(m.group(0)))
            i = end + 1 + len(hashes)
            continue
        if c == '"':
            i += 1
            while i < n and text[i] != '"':
                if text[i] == "\\":
                    i += 1
                i += 1
            i += 1
            continue
        if c == "'":
            # 文字リテラル('a', '\n', '\'')かライフタイム('a)か。閉じ ' が 2〜3 文字先にあれば文字リテラル
            m = re.match(r"'(\\.|[^\\'])'", text[i : i + 4])
            if m:
                i += len(m.group(0))
                continue
            i += 1
            continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                end = text.find("\n", i)
                block = text[start : end if end >= 0 else n]
                # 先頭行のインデントを揃える
                lines = block.split("\n")
                indent = len(lines[0]) - len(lines[0].lstrip())
                return "\n".join(l[indent:] if l.startswith(" " * indent) else l for l in lines)
        i += 1
    raise SystemExit(f"閉じ括弧が見つかりません: {start_marker!r}")


def main():
    modules = {}
    tests = {}
    for p in MODULE_PATHS:
        body, t = split_tests(read(os.path.join(SRC, p)))
        modules[p] = body + "\n"
        if t:
            tests[p] = t + "\n"
    steps = {}
    for s in RUN_STEPS:
        steps[s] = read(os.path.join(STEPS, "run", s + ".rs"))
    for s in MINI_STEPS:
        steps[s] = read(os.path.join(STEPS, s + ".rs"))
    excerpts = {}
    for key, p, marker in EXCERPTS:
        excerpts[key] = brace_block(read(os.path.join(SRC, p)), marker) + "\n"

    out = []
    out.append("// 自動生成。手で編集しない。")
    out.append("// 生成元: scratchpad の tinybrowser クレート(tools/emit_ts.py)。クレートの中身とサイトのコードを一致させるため、")
    out.append("// コードを直すときはクレート側を直して cargo test を通し、このファイルを作り直す。")
    out.append("/* eslint-disable */")
    out.append("export const RB_MODULES: Record<string, string> = " + json.dumps(modules, ensure_ascii=False, indent=2) + ";")
    out.append("export const RB_MODULE_TESTS: Record<string, string> = " + json.dumps(tests, ensure_ascii=False, indent=2) + ";")
    out.append("export const RB_STEPS: Record<string, string> = " + json.dumps(steps, ensure_ascii=False, indent=2) + ";")
    out.append("export const RB_EXCERPTS: Record<string, string> = " + json.dumps(excerpts, ensure_ascii=False, indent=2) + ";")
    out.append("")
    dest = os.environ.get("RB_TS_OUT", os.path.join(ROOT, "rust-browser-code.ts"))
    with open(dest, "w", encoding="utf-8") as f:
        f.write("\n".join(out))
    total = sum(len(v) for v in modules.values()) + sum(len(v) for v in steps.values()) + sum(len(v) for v in excerpts.values())
    print("wrote", dest, f"{total/1024:.0f} KB of source", f"({len(modules)} modules, {len(steps)} steps, {len(excerpts)} excerpts)")


if __name__ == "__main__":
    main()
