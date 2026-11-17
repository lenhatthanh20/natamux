//! A rem size for one subtree. gpui reads rems from the element that draws a deferred menu, so a
//! menu sets its own rem (the UI font size); the diff sets the rem of its side's font.

use gpui::{
    AnyElement, App, Bounds, Div, Element, ElementId, GlobalElementId, InspectorElementId,
    InteractiveElement, IntoElement, LayoutId, ParentElement, Pixels, StyleRefinement, Styled,
    Window, div,
};

/// Lays out and paints `inner` with `rem` as the size of one rem.
pub struct RemScope<E> {
    rem: Pixels,
    inner: E,
}

/// The box call sites build: a `div` under its own rem size.
pub type WithRemSize = RemScope<Div>;

impl RemScope<Div> {
    pub fn new(rem: impl Into<Pixels>) -> Self {
        RemScope {
            rem: rem.into(),
            inner: div(),
        }
    }

    /// Stops the pointer from reaching what lies under the box.
    pub fn occlude(self) -> Self {
        RemScope {
            inner: self.inner.occlude(),
            ..self
        }
    }
}

impl<E: Styled> Styled for RemScope<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}

impl<E: ParentElement> ParentElement for RemScope<E> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(elements);
    }
}

impl<E: Element> IntoElement for RemScope<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

// Each phase runs the inner element's phase with the rem pushed, so every length in the subtree
// resolves against it.
impl<E: Element> Element for RemScope<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

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
        let Self { rem, inner } = self;
        window.with_rem_size(Some(*rem), |window| {
            inner.request_layout(id, inspector_id, window, cx)
        })
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let Self { rem, inner } = self;
        window.with_rem_size(Some(*rem), |window| {
            inner.prepaint(id, inspector_id, bounds, layout, window, cx)
        })
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Self { rem, inner } = self;
        window.with_rem_size(Some(*rem), |window| {
            inner.paint(id, inspector_id, bounds, layout, prepaint, window, cx)
        });
    }
}
