//! The saved choice of the Docs view: Rendered or Source (`docsSource` of `view.json`).

use ui::prelude::*;

use super::Dashboard;

impl Dashboard {
    pub fn set_docs_source(&mut self, on: bool, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.docs_source = on);
    }
}
