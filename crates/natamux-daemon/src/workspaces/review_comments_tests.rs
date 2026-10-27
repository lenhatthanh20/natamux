use natamux_git::test_support::TempDir;

use super::*;

fn comment(id: &str, path: &str, start: u32) -> ReviewComment {
    ReviewComment {
        id: id.to_string(),
        path: path.to_string(),
        side: CommentSide::New,
        start,
        end: start,
        quote: "x".to_string(),
        body: "Rename".to_string(),
        state: CommentState::Draft,
        created_at: "2026-10-27T00:00:00.000Z".to_string(),
        sent_at: None,
        resolved_at: None,
        outdated: false,
    }
}

#[test]
fn orders_by_path_then_line_then_id_number() {
    let list = sorted(vec![
        comment("c10", "b.ts", 1),
        comment("c2", "b.ts", 1),
        comment("c3", "a.ts", 9),
        comment("c4", "a.ts", 2),
    ]);

    let ids: Vec<&str> = list.iter().map(|comment| comment.id.as_str()).collect();

    assert_eq!(ids, ["c4", "c3", "c2", "c10"]);
}

#[test]
fn a_missing_file_has_no_comments_and_a_saved_one_reads_back() {
    let dir = TempDir::new("review-comments");
    let path = dir.path().join("task").join(STORE_FILE);
    assert_eq!(load(&path).unwrap(), ReviewCommentsFile::default());

    let file = ReviewCommentsFile {
        next: 4,
        comments: vec![comment("c4", "a.ts", 2)],
    };
    save(&path, &file).unwrap();

    assert_eq!(load(&path).unwrap(), file);
    assert!(
        !dir.path()
            .join("task")
            .join("review-comments.json.tmp")
            .exists()
    );
}
