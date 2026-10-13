//! 描画。箱の木を「表示リスト(display list)」= 描く命令の列に変え、さらに文字のキャンバスに描く。
//!
//! 本物のブラウザも、レイアウトの結果を直接画面に描くのではなく、
//! 一度「何をどこに描くか」の命令列(display list / paint record)にしてから GPU に渡す。
//! この実装はウィンドウを持たないので、命令列を 1 文字 = 8×16px のセルからなるキャンバスに描いて
//! 標準出力に出す。

use crate::renderer::layout::{BoxKind, LayoutBox};
use crate::renderer::style::{Color, TextDecoration};

#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    /// 背景の四角
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    },
    /// 1 行分の文字列
    Text {
        x: f32,
        y: f32,
        text: String,
        color: Color,
        font_size: f32,
        underline: bool,
    },
}

/// 箱の木 → 表示リスト。親の背景を先に、文字を後に(後に描いたものが上に来る)
pub fn paint(root: &LayoutBox) -> Vec<DisplayItem> {
    let mut items = Vec::new();
    paint_box(root, &mut items);
    items
}

fn paint_box(b: &LayoutBox, items: &mut Vec<DisplayItem>) {
    match &b.kind {
        BoxKind::Block | BoxKind::Inline => {
            if let Some(bg) = b.style.background_color {
                items.push(DisplayItem::Rect {
                    x: b.x,
                    y: b.y,
                    width: b.width,
                    height: b.height,
                    color: bg,
                });
            }
        }
        BoxKind::Text(_) => {
            for line in &b.lines {
                items.push(DisplayItem::Text {
                    x: line.x,
                    y: line.y,
                    text: line.text.clone(),
                    color: b.style.color,
                    font_size: b.style.font_size,
                    underline: b.style.text_decoration == TextDecoration::Underline,
                });
            }
        }
    }
    for c in &b.children {
        paint_box(c, items);
    }
}

/// 1 セルの大きさ(px)。16px の文字 1 つが 1 セルに収まる想定
pub const CELL_WIDTH: f32 = 8.0;
pub const CELL_HEIGHT: f32 = 24.0; // 16px × 行の高さの比 1.5

/// 表示リストを文字のキャンバスに描く。幅は px で与える
pub fn render_ascii(items: &[DisplayItem], width_px: f32) -> String {
    let cols = (width_px / CELL_WIDTH).ceil() as usize;
    let bottom = items
        .iter()
        .map(|i| match i {
            DisplayItem::Rect { y, height, .. } => y + height,
            DisplayItem::Text { y, font_size, .. } => y + font_size * 1.5,
        })
        .fold(0.0_f32, f32::max);
    let rows = (bottom / CELL_HEIGHT).ceil().max(1.0) as usize;
    let mut grid = vec![vec![' '; cols]; rows];

    for item in items {
        match item {
            DisplayItem::Rect { x, y, width, height, color } => {
                if *color == Color::WHITE {
                    continue;
                }
                let (c0, r0) = (cell(*x, CELL_WIDTH), cell(*y, CELL_HEIGHT));
                let c1 = cell(x + width, CELL_WIDTH).min(cols);
                let r1 = cell(y + height, CELL_HEIGHT).min(rows);
                for row in grid.iter_mut().take(r1).skip(r0) {
                    for ch in row.iter_mut().take(c1).skip(c0) {
                        *ch = '░';
                    }
                }
            }
            DisplayItem::Text { x, y, text, underline, .. } => {
                let (mut c, r) = (cell(*x, CELL_WIDTH), cell(*y, CELL_HEIGHT));
                if r >= rows {
                    continue;
                }
                for ch in text.chars() {
                    if c >= cols {
                        break;
                    }
                    grid[r][c] = if *underline && ch == ' ' { '_' } else { ch };
                    c += 1;
                }
            }
        }
    }

    let mut out = String::new();
    out.push('┌');
    out.push_str(&"─".repeat(cols));
    out.push_str("┐\n");
    for row in grid {
        out.push('│');
        out.extend(row);
        out.push_str("│\n");
    }
    out.push('└');
    out.push_str(&"─".repeat(cols));
    out.push('┘');
    out
}

fn cell(px: f32, size: f32) -> usize {
    (px / size).floor().max(0.0) as usize
}

/// 表示リストを 1 行 1 命令で並べる(学習用)
pub fn dump_display_list(items: &[DisplayItem]) -> String {
    items
        .iter()
        .map(|i| match i {
            DisplayItem::Rect { x, y, width, height, color } => {
                format!("Rect  x={x:.0} y={y:.0} w={width:.0} h={height:.0} color={}", color.hex())
            }
            DisplayItem::Text { x, y, text, color, font_size, underline } => format!(
                "Text  x={x:.0} y={y:.0} {text:?} color={} size={font_size:.0}px{}",
                color.hex(),
                if *underline { " underline" } else { "" }
            ),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::parser::CssParser;
    use crate::html::parser::HtmlParser;
    use crate::renderer::layout::layout_document;

    fn items(html: &str, css: &str) -> Vec<DisplayItem> {
        let doc = HtmlParser::parse(html);
        let root = layout_document(&doc, &CssParser::parse(css)).unwrap();
        paint(&root)
    }

    #[test]
    fn background_then_text() {
        let it = items("<p>hi</p>", "p { background-color: yellow; color: red }");
        assert!(matches!(it[0], DisplayItem::Rect { .. }));
        match &it[1] {
            DisplayItem::Text { text, color, .. } => {
                assert_eq!(text, "hi");
                assert_eq!(*color, Color::from_name("red").unwrap());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn link_is_underlined() {
        let it = items("<a>x</a>", "");
        assert!(matches!(it[0], DisplayItem::Text { underline: true, .. }));
    }

    #[test]
    fn ascii_canvas_places_text() {
        let it = items("<p>ab</p><p>cd</p>", "");
        let s = render_ascii(&it, 64.0); // 8 セル幅
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[1], "│ab      │");
        assert_eq!(lines[2], "│cd      │");
    }
}
