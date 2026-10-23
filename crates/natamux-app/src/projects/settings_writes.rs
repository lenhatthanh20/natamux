//! What the footer and the conflict box of Project settings do (`ProjectSettingsDialog.tsx`):
//! Save, Overwrite, Reload, Reset to default, Import and Export. Import and Export change only the
//! draft or the picked file; only Save and Overwrite write `.natamux.json`.

use std::path::{Path, PathBuf};

use gpui::{DismissEvent, PathPromptOptions};
use natamux_proto::stages::catalog::default_pipeline;
use serde_json::{Map, Value};
use ui::prelude::*;

use super::project_settings_dialog::ProjectSettingsDialog;
use super::settings_tab_rules::reset_ask;
use super::settings_text::{MAX_IMPORT_BYTES, base_of, import_question};
use crate::app::settings_reads::SettingsReads;
use crate::app::source::reason;
use crate::components::confirm::Confirm;
use crate::components::toast_rules::ToastTone;
use crate::stages::editor_draft::{format_settings, merge_changes, parse_draft};
use crate::stages::editor_fields::shell_fields;

impl ProjectSettingsDialog {
    /// Save and Mod+Enter: ignored while reading, saving, with problems, or with nothing changed.
    /// A broken file asks first, since the screen started from the defaults.
    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(base), Some(stages)) = (&self.base, &self.stages) else {
            return;
        };
        if self.saving || self.save_blocked(cx).is_some() {
            return;
        }
        let Some(raw) = stages.editor.read(cx).raw().cloned() else {
            return;
        };
        let version = base.version.clone();
        let Some(broken) = base.broken.clone() else {
            self.send(raw, version, cx);
            return;
        };
        let path = base.path.clone();
        let this = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            Confirm::new(
                "Replace the file?",
                format!(
                    "{path} has a problem, so this screen started from the defaults. Saving \
                     replaces your file. Save anyway? ({broken})"
                ),
                "Save anyway",
                move |_, cx| {
                    this.update(cx, |this, cx| this.send(raw.clone(), version.clone(), cx))
                        .ok();
                },
            )
            .open_over(window, cx);
        });
    }

    /// `PUT /repos/:id/settings`. Without `version` the file is replaced even when it changed.
    fn send(&mut self, raw: Map<String, Value>, version: Option<String>, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        self.saving = true;
        self.conflict = false;
        self.problem = None;
        let answer = source.save_settings(&self.repo_id, raw, version.clone());
        cx.spawn(async move |this, cx| {
            let answer = answer.await;
            this.update(cx, |this, cx| {
                this.saving = false;
                match answer {
                    Ok(_) => this.saved(cx),
                    Err(error) if error.is_conflict() && version.is_some() => this.conflict = true,
                    Err(error) => this.problem = Some(reason(&error)),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn saved(&mut self, cx: &mut Context<Self>) {
        let text = format!(
            "Saved .natamux.json of {}. Commit it to share it.",
            self.repo_name
        );
        self.dashboard.update(cx, |dashboard, cx| {
            dashboard.toast(&text, ToastTone::Success, None, cx)
        });
        if let Some(reads) = SettingsReads::global(cx) {
            let repo = self.repo_id.clone();
            reads.update(cx, |reads, cx| reads.refresh(&repo, cx));
        }
        cx.emit(DismissEvent);
    }

    /// Saves the edits over the file as it is now: keys the person did not change keep the
    /// values of the new file, so the other client's edits are not lost.
    pub(super) fn overwrite(&mut self, cx: &mut Context<Self>) {
        let (Some(base), Some(stages)) = (&self.base, &self.stages) else {
            return;
        };
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        if self.saving {
            return;
        }
        let start = parse_draft(&base.text).ok();
        let Some(draft) = stages.editor.read(cx).raw().cloned() else {
            return;
        };
        let fresh = source.load_settings(&self.repo_id);
        cx.spawn(async move |this, cx| {
            let fresh = fresh.await.ok();
            this.update(cx, |this, cx| {
                // A file that cannot be read keeps none of its keys: the edits replace it.
                let now = fresh
                    .as_ref()
                    .map(base_of)
                    .filter(|now| now.broken.is_none())
                    .and_then(|now| parse_draft(&now.text).ok());
                let raw = match (&start, &now) {
                    (Some(start), Some(now)) => merge_changes(&start.raw, &draft, &now.raw),
                    _ => draft,
                };
                let version = fresh.and_then(|loaded| loaded.version);
                this.send(raw, version, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Takes the file as it is now and drops the edits.
    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let fresh = source.load_settings(&self.repo_id);
        cx.spawn(async move |this, cx| {
            let Ok(fresh) = fresh.await else {
                return;
            };
            this.update(cx, |this, cx| {
                let base = base_of(&fresh);
                if let Some(stages) = &this.stages {
                    let text = base.text.clone();
                    stages
                        .editor
                        .update(cx, |editor, cx| editor.set_text(&text, cx));
                }
                this.base = Some(base);
                this.conflict = false;
                this.problem = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Puts the default stages back in the draft, after asking. Text that cannot be read is
    /// replaced by `{}`, which is the default settings.
    pub(super) fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.stages.as_ref().map(|stages| stages.editor.clone()) else {
            return;
        };
        let ask = reset_ask(self.draft_ok(cx));
        window.defer(cx, move |window, cx| {
            Confirm::new("Reset to default", ask.question, "Reset", move |_, cx| {
                editor.update(cx, |editor, cx| match ask.text {
                    Some(text) => editor.set_text(text, cx),
                    None => editor.edit(default_pipeline(), cx),
                });
            })
            .safe()
            .open_over(window, cx);
        });
    }

    /// Import…: a `.json` file into the draft only. A file that does not parse goes in too; the
    /// JSON tab shows its error.
    pub(super) fn import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let read = cx
                .background_executor()
                .spawn(async move { read_import(&path) })
                .await;
            this.update_in(cx, |this, window, cx| this.took_import(read, window, cx))
                .ok();
        })
        .detach();
    }

    fn took_import(
        &mut self,
        read: Result<(String, String), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (name, text) = match read {
            Ok(read) => read,
            Err(problem) => {
                self.problem = Some(problem);
                cx.notify();
                return;
            }
        };
        let Some(editor) = self.stages.as_ref().map(|stages| stages.editor.clone()) else {
            return;
        };
        // A file that does not parse lists nothing; the JSON tab shows all of it.
        let fields =
            parse_draft(&text).map_or_else(|_| Vec::new(), |draft| shell_fields(&draft.steps));
        let question = import_question(&name, self.dirty(cx), &fields);
        let dashboard = self.dashboard.clone();
        let apply = move |cx: &mut App| {
            editor.update(cx, |editor, cx| editor.set_text(&text, cx));
            let said = format!("Imported {name}. Review it, then click Save.");
            dashboard.update(cx, |dashboard, cx| {
                dashboard.toast(&said, ToastTone::Info, None, cx)
            });
        };
        match question {
            None => apply(cx),
            Some(question) => {
                Confirm::new("Import settings", question, "Import", move |_, cx| {
                    apply(cx)
                })
                .safe()
                .open_over(window, cx);
            }
        }
    }

    /// Export: the draft into a file the person picks, as `<project>.natamux.json`.
    pub(super) fn export(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(raw) = self
            .stages
            .as_ref()
            .and_then(|stages| stages.editor.read(cx).raw().cloned())
        else {
            return;
        };
        let text = format!("{}\n", format_settings(&raw));
        let home = std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from);
        let name = format!("{}.natamux.json", self.repo_name);
        let path = cx.prompt_for_new_path(&home, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = path.await else {
                return;
            };
            let target = path.clone();
            let written = cx
                .background_executor()
                .spawn(async move { std::fs::write(&target, text) })
                .await;
            this.update(cx, |this, cx| match written {
                Ok(()) => {
                    let said = format!("Exported {}.", file_name(&path));
                    this.dashboard.update(cx, |dashboard, cx| {
                        dashboard.toast(&said, ToastTone::Info, None, cx)
                    });
                }
                Err(error) => {
                    this.problem = Some(format!("Could not write {}: {error}", file_name(&path)));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// The name and text of an import, or why it cannot be read.
fn read_import(path: &Path) -> Result<(String, String), String> {
    let name = file_name(path);
    let size = std::fs::metadata(path).map_or(0, |meta| meta.len());
    if size > MAX_IMPORT_BYTES as u64 {
        return Err(format!("{name} is too big for a settings file."));
    }
    let text = std::fs::read_to_string(path).map_err(|_| format!("Could not read {name}"))?;
    Ok((name, text))
}
