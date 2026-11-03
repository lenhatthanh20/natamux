//! What almost every view imports: gpui's names, the kit's own widgets, and Zed's enums and
//! traits that the Zed widgets not yet replaced still take (plan 6.7.1, S8.2).

pub use zed_ui::prelude::*;

pub use crate::{
    Button, ButtonCommon, DynamicSpacing, Headline, HeadlineSize, Icon, IconButton, Label,
    LabelCommon, LineHeightStyle, SelectableButton, h_flex, rems_from_px, v_flex,
};
