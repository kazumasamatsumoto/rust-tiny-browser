//! レンダラ。DOM と CSSOM から、要素ごとの計算値(style)→ 箱の木と座標(layout)→ 表示リスト(paint)。
pub mod layout;
pub mod paint;
pub mod style;
