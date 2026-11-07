//! A small declarative layer over taffy. `row![]` / `column![]` build a
//! *description* of the layout tree as a plain value; [`build`] then walks
//! that description once and produces the real `TaffyTree`.
//!
//! The description step is what lets the macros nest as ordinary expressions:
//! if `row![]` returned a `TaffyTree` of its own, nesting would require
//! merging two trees, which taffy has no API for.
//!
//! Kept inside `m3_test` on purpose — `ui_core` / `ui_widget` / `m3_widget`
//! stay layout-engine-agnostic (`docs/widgets.md`). Promote this to its own
//! crate if the bar ever needs it.

use std::collections::HashMap;

use taffy::prelude::*;
use ui_core::geometry::LayoutRect;

use m3_widget::Widget;

/// A widget paired with the taffy node that drives its layout. The widget is
/// already fully constructed — only its rect is missing, and the caller pushes
/// that in with `Widget::set_layout_rect` once `compute_layout` has run. This
/// is the same path a resize takes, so there is no separate first-time route.
pub struct PendingWidget {
    pub node: NodeId,
    pub widget: Box<dyn Widget>,
}

/// A section caption, drawn just above its node's resolved top edge. Its
/// screen position is re-derived from the node on every relayout, resize
/// included.
pub struct Caption {
    pub label: &'static str,
    pub node: NodeId,
}

enum Kind {
    Leaf(Box<dyn Widget>),
    Container(Vec<LayoutNode>),
}

/// One node of a not-yet-realised layout tree. Build these with [`leaf`],
/// [`row!`](crate::row) / [`column!`](crate::column) (or
/// [`LayoutNode::row`] / [`LayoutNode::column`] when the children come from an
/// iterator), then hand the root to [`build`].
pub struct LayoutNode {
    style: Style,
    caption: Option<&'static str>,
    kind: Kind,
}

impl LayoutNode {
    /// A flex row. Children keep their own height and sit against the top
    /// edge, matching how the gallery lines up controls of differing heights.
    pub fn row(children: Vec<LayoutNode>) -> Self {
        Self {
            style: Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: Some(AlignItems::FLEX_START),
                ..Default::default()
            },
            caption: None,
            kind: Kind::Container(children),
        }
    }

    /// A flex column.
    pub fn column(children: Vec<LayoutNode>) -> Self {
        Self {
            style: Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            caption: None,
            kind: Kind::Container(children),
        }
    }

    /// Spacing between children, applied along the node's own main axis.
    pub fn gap(mut self, gap: f32) -> Self {
        self.style.gap = match self.style.flex_direction {
            FlexDirection::Column | FlexDirection::ColumnReverse => Size {
                width: length(0.0),
                height: length(gap),
            },
            _ => Size {
                width: length(gap),
                height: length(0.0),
            },
        };
        self
    }

    /// Labels this node; [`build`] returns a [`Caption`] for it. Any node may
    /// be captioned, not just top-level sections.
    pub fn caption(mut self, caption: &'static str) -> Self {
        self.caption = Some(caption);
        self
    }

    pub fn padding(mut self, padding: Rect<LengthPercentage>) -> Self {
        self.style.padding = padding;
        self
    }
}

/// A widget occupying a fixed-size slot.
///
/// The size is given explicitly rather than taken from `Widget::measure`:
/// several widgets (`Divider`, `Slider`, `TextField`, `ListItem`) don't
/// override `measure` and would report 0×0 before their first layout, and
/// `NavigationRail::measure` returns its *animated* width, which changes frame
/// to frame.
pub fn leaf(width: f32, height: f32, widget: impl Widget + 'static) -> LayoutNode {
    LayoutNode {
        style: Style {
            size: Size {
                width: length(width),
                height: length(height),
            },
            ..Default::default()
        },
        caption: None,
        kind: Kind::Leaf(Box::new(widget)),
    }
}

/// A flex row of children. `row![a, b, c]`; use [`LayoutNode::row`] directly
/// when the children come from an iterator.
#[macro_export]
macro_rules! row {
    ($($child:expr),* $(,)?) => {
        $crate::layout::LayoutNode::row(vec![$($child),*])
    };
}

/// A flex column of children. See [`row!`].
#[macro_export]
macro_rules! column {
    ($($child:expr),* $(,)?) => {
        $crate::layout::LayoutNode::column(vec![$($child),*])
    };
}

/// Realises a described tree into a `TaffyTree`, collecting every widget and
/// caption along with the node each is bound to. Widgets come back in
/// depth-first description order, which is also the order they should be
/// drawn in.
///
/// The tree's context type is `()`: a real context is only needed for
/// `compute_layout_with_measure`, which this layer deliberately doesn't use.
pub fn build(root: LayoutNode) -> (Vec<PendingWidget>, Vec<Caption>, TaffyTree<()>, NodeId) {
    let mut tree = TaffyTree::new();
    let mut widgets = Vec::new();
    let mut captions = Vec::new();
    let node = insert(&mut tree, root, &mut widgets, &mut captions);
    (widgets, captions, tree, node)
}

fn insert(
    tree: &mut TaffyTree<()>,
    node: LayoutNode,
    widgets: &mut Vec<PendingWidget>,
    captions: &mut Vec<Caption>,
) -> NodeId {
    let LayoutNode {
        style,
        caption,
        kind,
    } = node;
    let id = tree.new_leaf(style).unwrap();

    match kind {
        Kind::Leaf(widget) => widgets.push(PendingWidget { node: id, widget }),
        Kind::Container(children) => {
            for child in children {
                let child_id = insert(tree, child, widgets, captions);
                tree.add_child(id, child_id).unwrap();
            }
        }
    }

    if let Some(label) = caption {
        captions.push(Caption { label, node: id });
    }
    id
}

/// Accumulates each node's absolute `LayoutRect` (position and size), since
/// `Layout::location` is relative to the immediate parent. This is the one
/// place taffy's geometry is converted into `ui_core`'s layout-agnostic
/// `LayoutRect` — `ui_core` and `m3_widget` know nothing about taffy.
pub fn resolve_layout_rects(
    tree: &TaffyTree<()>,
    node: NodeId,
    origin: (f32, f32),
    out: &mut HashMap<NodeId, LayoutRect>,
) {
    let layout = tree.layout(node).unwrap();
    let x = origin.0 + layout.location.x;
    let y = origin.1 + layout.location.y;
    out.insert(
        node,
        LayoutRect::new(x, y, layout.size.width, layout.size.height),
    );
    for child in tree.children(node).unwrap() {
        resolve_layout_rects(tree, child, (x, y), out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::font::FontBook;
    use ui_core::scheme::ColorTheme;

    struct Stub(LayoutRect);

    impl Stub {
        fn new() -> Self {
            Self(LayoutRect::new(0.0, 0.0, 0.0, 0.0))
        }
    }

    impl Widget for Stub {
        fn draw(&mut self, _: &skia_safe::Canvas, _: &ColorTheme, _: &FontBook) -> bool {
            false
        }
        fn set_layout_rect(&mut self, rect: LayoutRect) {
            self.0 = rect;
        }
        fn layout_rect(&self) -> LayoutRect {
            self.0
        }
    }

    /// Leaf widths double as identity: if `build` reorders or misparents
    /// anything, the widths come back in the wrong order.
    #[test]
    fn build_preserves_order_and_nesting() {
        let root = column![
            row![leaf(10.0, 10.0, Stub::new()), leaf(20.0, 10.0, Stub::new())].caption("first"),
            column![
                leaf(30.0, 10.0, Stub::new()),
                row![leaf(40.0, 10.0, Stub::new())],
            ],
            leaf(50.0, 10.0, Stub::new()),
        ]
        .gap(4.0);

        let (widgets, captions, mut tree, root_node) = build(root);

        assert_eq!(widgets.len(), 5);
        assert_eq!(captions.len(), 1);
        assert_eq!(captions[0].label, "first");

        tree.compute_layout(
            root_node,
            Size {
                width: AvailableSpace::MaxContent,
                height: AvailableSpace::MaxContent,
            },
        )
        .unwrap();

        let widths: Vec<f32> = widgets
            .iter()
            .map(|w| tree.layout(w.node).unwrap().size.width)
            .collect();
        assert_eq!(widths, vec![10.0, 20.0, 30.0, 40.0, 50.0]);

        // Root has three children: the captioned row, a column, and a leaf.
        let top = tree.children(root_node).unwrap();
        assert_eq!(top.len(), 3);
        assert_eq!(top[0], captions[0].node);
        assert_eq!(tree.children(top[0]).unwrap().len(), 2);
        assert_eq!(tree.children(top[1]).unwrap().len(), 2);
        assert_eq!(top[2], widgets[4].node);

        // Absolute rects accumulate down the tree: the last leaf sits below
        // the two sections plus two gaps.
        let mut rects = HashMap::new();
        resolve_layout_rects(&tree, root_node, (0.0, 0.0), &mut rects);
        assert_eq!(rects[&widgets[4].node].y, 10.0 + 4.0 + 20.0 + 4.0);
    }
}
