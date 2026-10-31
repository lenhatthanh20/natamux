//! T-MD-1 (plan 6.6, 7.6): what the Docs view draws for the rich doc of the shots
//! (`workspace-docs-rich`) at zoom 1.0 and 1.077, recorded on Zed's Markdown view. Rows: each
//! line laid out (its text after smart punctuation, font size and fonts), then what was painted
//! (quads, text runs, icons, underlines) in px from the view's top left. The own view (S5.2) must
//! give the same rows. The window is the kit golden's (`components/kit_metrics.rs`): no real
//! fonts, every glyph is one box whose height names its font.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gpui::{AnyWindowHandle, size};
use serde_json::{Value, json};
use ui::prelude::*;

use super::docs_markdown::MarkdownDoc;
use crate::app::golden_files;
use crate::app::shots_text::RICH_TEXT;
use crate::components::kit_metrics::{CONFIGS, context, redraw};

const SET: &str = "markdown";
const NAME: &str = "rich";
/// Zoom 1.0 and 1.077 (`app/appearance_rules.rs`).
const CONFIG_NAMES: [&str; 2] = ["dark-default", "dark-large"];
/// Tall enough for the whole doc, so nothing is scrolled or clipped.
const WIDTH: f32 = 640.;
const HEIGHT: f32 = 2400.;

/// The view as the Docs tab holds it, under the UI font and text colour of the app's root.
struct Doc {
    doc: MarkdownDoc,
}

impl Render for Doc {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let font = theme_settings::setup_ui_font(window, cx);
        div()
            .size_full()
            .font(font)
            .text_color(cx.theme().colors().text)
            .child(self.doc.view(window, cx))
    }
}

fn rows() -> Vec<Value> {
    let mut rows = Vec::new();
    for config in CONFIGS.iter().filter(|c| CONFIG_NAMES.contains(&c.name)) {
        let (mut cx, capture, text) = context(config);
        let handle = cx
            .open_window(size(px(WIDTH), px(HEIGHT)), |_, cx| {
                cx.new(|cx| {
                    let doc = MarkdownDoc::new(cx);
                    doc.set_text(RICH_TEXT, cx);
                    Doc { doc }
                })
            })
            .expect("a window");
        let window: AnyWindowHandle = handle.into();
        cx.run_until_parked();
        redraw(&mut cx, window);
        let scale = cx
            .update_window(window, |_, window, _| window.scale_factor())
            .expect("the window is open");
        {
            let mut capture = capture.borrow_mut();
            capture.origin = (0., 0.);
            capture.scale = scale;
        }
        cx.capture_screenshot(window).expect("the recorder captures");
        let painted = capture.borrow_mut().painted.take().expect("a frame");
        let lines = std::mem::take(&mut *text.lines.lock().unwrap());
        for line in lines {
            rows.push(json!({"config": config.name, "kind": "line", "value": line}));
        }
        for part in ["quads", "shadows", "text", "icons", "underlines"] {
            for item in painted[part].as_array().expect("a list") {
                rows.push(json!({"config": config.name, "kind": part, "value": item}));
            }
        }
        for count in ["images", "paths"] {
            rows.push(json!({"config": config.name, "kind": count, "value": painted[count]}));
        }
    }
    rows
}

/// Writes `$NATAMUX_GOLDEN_OUT/markdown/rich.json` (`Tool: golden`, plan 8.4).
#[test]
#[ignore = "makes the golden file; run by the Tool: golden workflow"]
fn golden_docs_markdown() {
    golden_files::write(SET, NAME, &rows());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn the_docs_view_draws_what_the_golden_holds() {
    golden_files::check(SET, NAME, &rows());
}
