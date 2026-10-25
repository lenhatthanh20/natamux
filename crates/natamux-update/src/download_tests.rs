use std::io::Cursor;

use super::download::save_verified;

#[test]
fn saves_only_a_bounded_file_with_the_signed_hash() {
    let root = std::env::temp_dir().join(format!("natamux-download-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("folder");
    let file = root.join("asset.dmg");
    let hash = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    save_verified(Cursor::new(b"hello"), &file, hash, 5).expect("verified file");
    assert_eq!(std::fs::read(&file).expect("asset"), b"hello");
    std::fs::remove_file(&file).expect("remove asset");
    assert!(save_verified(Cursor::new(b"changed"), &file, hash, 10).is_err());
    assert!(!file.exists());
    assert!(save_verified(Cursor::new(b"hello"), &file, hash, 4).is_err());
    assert!(!file.exists());
    std::fs::remove_dir(root).expect("remove folder");
}

#[test]
fn a_crashed_partial_download_does_not_block_the_next_try() {
    let root = std::env::temp_dir().join(format!("natamux-download-retry-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("folder");
    let file = root.join("asset.dmg");
    std::fs::write(root.join("asset.dmg.part"), b"old partial").expect("partial");
    let hash = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    save_verified(Cursor::new(b"hello"), &file, hash, 5).expect("retry");
    assert_eq!(std::fs::read(&file).expect("asset"), b"hello");
    assert!(!root.join("asset.dmg.part").exists());
    std::fs::remove_file(file).expect("remove asset");
    std::fs::remove_dir(root).expect("remove folder");
}
