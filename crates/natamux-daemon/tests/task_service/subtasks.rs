//! The group 'subtasks'.

use super::*;

#[test]
fn makes_a_subtask_of_a_task() {
    let fx = Fixture::new();
    let parent = create(&fx.service, "Release 1.0");
    let child = create_with(
        &fx.service,
        json!({ "title": "Write the notes", "parentId": parent.id }),
    );

    assert_eq!(child.parent_id.as_deref(), Some(parent.id.as_str()));
    assert_eq!(create(&fx.service, "Top").parent_id, None);
}

#[test]
fn puts_a_subtask_in_the_project_of_its_parent_unless_it_names_one() {
    let fx = Fixture::new();
    let parent = create_with(
        &fx.service,
        json!({ "title": "Release 1.0", "repo": "shop" }),
    );

    let notes = create_with(
        &fx.service,
        json!({ "title": "Notes", "parentId": parent.id }),
    );
    let site = create_with(
        &fx.service,
        json!({ "title": "Site", "parentId": parent.id, "repo": "site" }),
    );

    assert_eq!(notes.repo.as_deref(), Some("shop"));
    assert_eq!(site.repo.as_deref(), Some("site"));
}

#[test]
fn refuses_an_unknown_parent_and_a_second_level() {
    let fx = Fixture::new();
    let parent = create(&fx.service, "Release 1.0");
    let child = create_with(
        &fx.service,
        json!({ "title": "Write the notes", "parentId": parent.id }),
    );
    let create_under = |parent: &str| {
        fx.service.create_task(
            Some(&json!({ "title": "x", "parentId": parent })),
            &[],
            true,
        )
    };

    assert!(err_text(create_under("T-0099")).contains("T-0099"));
    assert!(err_text(create_under(&child.id)).contains("subtask"));
}

#[test]
fn refuses_a_subtask_that_depends_on_its_parent() {
    let fx = Fixture::new();
    let parent = create(&fx.service, "Release 1.0");
    let other = create(&fx.service, "Other");
    let input = json!({ "title": "Notes", "parentId": parent.id, "dependsOn": [parent.id] });
    assert_eq!(
        err_text(fx.service.create_task(Some(&input), &[], true)),
        "A subtask cannot depend on its parent"
    );

    let child = create_with(
        &fx.service,
        json!({ "title": "Notes", "parentId": parent.id }),
    );
    assert_eq!(
        err_text(
            fx.service
                .update_task(&child.id, Some(&json!({ "dependsOn": [parent.id] })))
        ),
        "A subtask cannot depend on its parent"
    );
    let updated = fx
        .service
        .update_task(&child.id, Some(&json!({ "dependsOn": [other.id] })))
        .unwrap();
    assert_eq!(updated.depends_on, [other.id]);
}
