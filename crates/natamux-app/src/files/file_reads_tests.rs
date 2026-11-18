use natamux_proto::workspaces::changes::ChangeStatus;
use natamux_proto::workspaces::files::EntryKind;

use super::*;

fn entry(name: &str, kind: EntryKind) -> TreeEntry {
    TreeEntry {
        name: name.to_string(),
        kind,
    }
}

fn change(path: &str) -> ChangeEntry {
    ChangeEntry {
        path: path.to_string(),
        status: ChangeStatus::Modified,
        original_path: None,
        added: None,
        removed: None,
    }
}

fn root_read() -> FileReads {
    let mut reads = FileReads::default();
    reads.apply_folder("", Ok(vec![entry("src", EntryKind::Folder)]));
    reads
}

#[test]
fn the_rows_are_built_once_until_something_changes() {
    let mut reads = root_read();
    let open = BTreeSet::new();
    let first = reads.rows(&open);
    assert!(Rc::ptr_eq(&first, &reads.rows(&open)));

    reads.forget_rows();
    let second = reads.rows(&open);
    assert!(!Rc::ptr_eq(&first, &second));
    assert_eq!(*first, *second);

    reads.apply_folder("src", Ok(vec![entry("a.ts", EntryKind::File)]));
    assert!(!Rc::ptr_eq(&second, &reads.rows(&open)));
}

#[test]
fn a_failed_read_keeps_the_rows() {
    let mut reads = root_read();
    let open = BTreeSet::new();
    let first = reads.rows(&open);
    reads.apply_folder("src", Err("gone".to_string()));
    assert!(Rc::ptr_eq(&first, &reads.rows(&open)));
}

#[test]
fn the_marks_are_worked_out_when_the_changes_arrive() {
    let mut reads = FileReads::default();
    assert!(reads.apply_changes(&[change("src/lib/b.ts")]));
    assert_eq!(
        reads.marks.files.get("src/lib/b.ts"),
        Some(&ChangeStatus::Modified)
    );
    assert!(reads.marks.folders.contains("src/lib"));
}

#[test]
fn the_same_changes_again_report_no_difference() {
    let mut reads = FileReads::default();
    reads.apply_changes(&[change("a.ts")]);
    assert!(!reads.apply_changes(&[change("a.ts")]));
    assert!(reads.apply_changes(&[]));
}
