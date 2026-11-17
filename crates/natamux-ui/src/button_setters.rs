//! The setters `Button` and `IconButton` pass on to the `ButtonLike` they are built on.

/// Gives a widget built on a `ButtonLike`, kept in its field `$inner`, the setters every button
/// takes. Each one is passed on to that `ButtonLike`.
macro_rules! forward_button_setters {
    ($widget:ty, $inner:ident) => {
        impl $widget {
            fn on_inner(
                mut self,
                change: impl FnOnce(
                    $crate::button_like::ButtonLike,
                ) -> $crate::button_like::ButtonLike,
            ) -> Self {
                self.$inner = change(self.$inner);
                self
            }
        }

        impl $crate::traits::Disableable for $widget {
            fn disabled(self, disabled: bool) -> Self {
                self.on_inner(|b| $crate::traits::Disableable::disabled(b, disabled))
            }
        }

        impl $crate::traits::Toggleable for $widget {
            fn toggle_state(self, selected: bool) -> Self {
                self.on_inner(|b| $crate::traits::Toggleable::toggle_state(b, selected))
            }
        }

        impl $crate::traits::Clickable for $widget {
            fn on_click(
                self,
                handler: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
            ) -> Self {
                self.on_inner(|b| $crate::traits::Clickable::on_click(b, handler))
            }

            fn cursor_style(self, cursor_style: gpui::CursorStyle) -> Self {
                self.on_inner(|b| $crate::traits::Clickable::cursor_style(b, cursor_style))
            }
        }

        impl $crate::traits::FixedWidth for $widget {
            fn width(self, width: impl Into<gpui::DefiniteLength>) -> Self {
                self.on_inner(|b| $crate::traits::FixedWidth::width(b, width))
            }

            fn full_width(self) -> Self {
                self.on_inner($crate::traits::FixedWidth::full_width)
            }
        }

        impl $crate::button_like::ButtonCommon for $widget {
            fn id(&self) -> &gpui::ElementId {
                $crate::button_like::ButtonCommon::id(&self.$inner)
            }

            fn style(self, style: $crate::button_like::ButtonStyle) -> Self {
                self.on_inner(|b| $crate::button_like::ButtonCommon::style(b, style))
            }

            fn size(self, size: $crate::button_like::ButtonSize) -> Self {
                self.on_inner(|b| $crate::button_like::ButtonCommon::size(b, size))
            }

            fn tooltip(
                self,
                tooltip: impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static,
            ) -> Self {
                self.on_inner(|b| $crate::button_like::ButtonCommon::tooltip(b, tooltip))
            }

            fn tab_index(self, tab_index: impl Into<isize>) -> Self {
                self.on_inner(|b| $crate::button_like::ButtonCommon::tab_index(b, tab_index))
            }

            fn track_focus(self, focus_handle: &gpui::FocusHandle) -> Self {
                self.on_inner(|b| $crate::button_like::ButtonCommon::track_focus(b, focus_handle))
            }
        }
    };
}

pub(crate) use forward_button_setters;
