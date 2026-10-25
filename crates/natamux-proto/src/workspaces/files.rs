//! The files explorer and editor routes (`packages/core/src/git/changes.ts`: `treeEntrySchema`,
//! `workspaceFileSchema`; `http/routes/workspaces.ts`).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Folder,
}

/// One row of a folder.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct TreeEntry {
    pub name: String,
    pub kind: EntryKind,
}

/// The answer of `GET /tasks/:id/tree`: folders first, then files, each by name.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct TreeList {
    #[serde(default)]
    pub entries: Vec<TreeEntry>,
}

/// A file of the worktree as the editor opens it. `text` is None for a binary or too large file.
/// `version` is the sha256 of the bytes; a save sends it back in `If-Match`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFile {
    pub path: String,
    pub text: Option<String>,
    pub binary: bool,
    pub too_large: bool,
    pub version: String,
}

/// The answer of `PUT /tasks/:id/file`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct SavedFile {
    pub path: String,
    pub version: String,
}

/// The body of `PUT /tasks/:id/file` and of `PUT /tasks/:id/files/:name`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct FileText {
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_folder_like_the_daemon_sends_it() {
        let list: TreeList = serde_json::from_str(
            r#"{"entries":[{"name":"src","kind":"folder"},{"name":"a.ts","kind":"file"}]}"#,
        )
        .unwrap();
        assert_eq!(list.entries[0].kind, EntryKind::Folder);
        assert_eq!(list.entries[1].name, "a.ts");
        assert_eq!(list.entries[1].kind, EntryKind::File);
    }

    #[test]
    fn reads_a_text_file_and_a_binary_one() {
        let text: WorkspaceFile = serde_json::from_str(
            r#"{"path":"a.ts","text":"x","binary":false,"tooLarge":false,"version":"v1"}"#,
        )
        .unwrap();
        assert_eq!(text.text.as_deref(), Some("x"));
        assert!(!text.too_large);
        let big: WorkspaceFile = serde_json::from_str(
            r#"{"path":"b.json","text":null,"binary":false,"tooLarge":true,"version":""}"#,
        )
        .unwrap();
        assert_eq!((big.text, big.too_large), (None, true));
    }

    #[test]
    fn reads_the_answer_of_a_save_and_sends_the_text() {
        let saved: SavedFile =
            serde_json::from_str(r#"{"path":"a.ts","version":"v2","x":1}"#).unwrap();
        assert_eq!(saved.version, "v2");
        let body = FileText { text: "y".into() };
        assert_eq!(serde_json::to_string(&body).unwrap(), r#"{"text":"y"}"#);
    }
}
