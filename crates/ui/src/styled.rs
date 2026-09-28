//! What components share: the size scale, the `Sizable` / `Disableable` /
//! `Selectable` builder traits, and layout shorthands.

use gpui::{Div, Pixels, Styled, div, px};

/// Control sizes. `Medium` is the macOS default (26pt controls).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

impl Size {
    /// Height of a button or single-line control.
    pub fn control_height(self) -> Pixels {
        match self {
            Size::Small => px(20.),
            Size::Medium => px(26.),
            Size::Large => px(32.),
        }
    }

    /// Label text inside a control.
    pub fn text_size(self) -> Pixels {
        match self {
            Size::Small => px(11.),
            Size::Medium => px(12.),
            Size::Large => px(13.),
        }
    }

    /// An icon inside a control.
    pub fn icon_size(self) -> Pixels {
        match self {
            Size::Small => px(12.),
            Size::Medium => px(14.),
            Size::Large => px(16.),
        }
    }

    /// Horizontal padding of a labelled control.
    pub fn padding_x(self) -> Pixels {
        match self {
            Size::Small => px(8.),
            Size::Medium => px(10.),
            Size::Large => px(12.),
        }
    }
}

pub trait Sizable: Sized {
    fn with_size(self, size: Size) -> Self;

    fn small(self) -> Self {
        self.with_size(Size::Small)
    }

    fn large(self) -> Self {
        self.with_size(Size::Large)
    }
}

/// A disabled control shows dimmed, ignores clicks and doesn't let them reach
/// its parents.
pub trait Disableable {
    fn disabled(self, disabled: bool) -> Self;
}

pub trait Selectable {
    fn selected(self, selected: bool) -> Self;
}

pub trait StyledExt: Styled + Sized {
    /// A row, children centred vertically.
    fn h_flex(self) -> Self {
        self.flex().flex_row().items_center()
    }

    /// A column.
    fn v_flex(self) -> Self {
        self.flex().flex_col()
    }
}

impl<E: Styled> StyledExt for E {}

pub fn h_flex() -> Div {
    div().h_flex()
}

pub fn v_flex() -> Div {
    div().v_flex()
}
