//! `describe('onCreated')`.

use super::*;

#[test]
fn is_told_about_a_new_workspace_and_its_failure_does_not_undo_the_workspace() {
    let fx = Fx::new();
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let told = Arc::clone(&seen);
    let watched = fx.build(|options| {
        options.on_created = Some(Arc::new(move |task| {
            told.lock()
                .unwrap()
                .push(task.workspace.clone().unwrap_or_default());
            panic!("setup failed");
        }));
    });
    let task = fx.task("Fix the bug");

    let created = watched.create(&task.id, None).unwrap();

    let workspace = created.workspace.clone().unwrap();
    assert_eq!(*seen.lock().unwrap(), std::slice::from_ref(&workspace));
    assert!(Path::new(&workspace).exists());
}
