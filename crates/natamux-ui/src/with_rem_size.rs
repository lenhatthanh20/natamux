//! A box that sets the rem size of its children. A menu drawn deferred would otherwise take the
//! rem size of whatever element draws it, so it sets the UI font size itself.

use gpui::{
    AnyElement, App, Bounds, Div, DivFrameState, Element, ElementId, GlobalElementId, Hitbox,
    InspectorElementId, InteractiveElement, IntoElement, LayoutId, ParentElement, Pixels,
    StyleRefinement, Styled, Window, div,
};

/// A `div` whose children are laid out and drawn with `rem_size` as one rem.
pub struct WithRemSize {
    inner: Div,
    rem_size: Pixels,
}

impl WithRemSize {
    pub fn new(rem_size: impl Into<Pixels>) -> Self {
        Self {
            inner: div(),
            rem_size: rem_size.into(),
        }
    }

    /// Blocks the pointer from reaching what is under the box.
    pub fn occlude(mut self) -> Self {
        self.inner = self.inner.occlude();
        self
    }
}

impl Styled for WithRemSize {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}

impl ParentElement for WithRemSize {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(elements)
    }
}

impl IntoElement for WithRemSize {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for WithRemSize {
    type RequestLayoutState = DivFrameState;
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        Element::id(&self.inner)
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        Element::source_location(&self.inner)
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let rem = self.rem_size;
        let inner = &mut self.inner;
        window.with_rem_size(Some(rem), |window| {
            inner.request_layout(id, inspector_id, window, cx)
        })
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let rem = self.rem_size;
        let inner = &mut self.inner;
        window.with_rem_size(Some(rem), |window| {
            inner.prepaint(id, inspector_id, bounds, state, window, cx)
        })
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let rem = self.rem_size;
        let inner = &mut self.inner;
        window.with_rem_size(Some(rem), |window| {
            inner.paint(id, inspector_id, bounds, state, hitbox, window, cx)
        })
    }
}
