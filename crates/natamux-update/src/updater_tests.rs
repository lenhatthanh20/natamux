use std::collections::HashMap;

use super::check::InstallKind;
use super::updater::{CheckOutcome, ReleaseFeed, check_now};

const TEST_KEY: &str = "RWRlDDYUMCwx+Oo1OxIoyy7UaN2jSLqaEfEX9pmWdYLOJWE1WCES0Jkf";
const TAG_URL: &str = "https://github.com/lenhatthanh20/natamux/releases/tag/v1.2.3";
const SUMS: &[u8] = include_bytes!("../tests/fixtures/minisign/SHA256SUMS.txt");
const SIGNATURE: &[u8] = include_bytes!("../tests/fixtures/minisign/SHA256SUMS.txt.legacy.minisig");

struct Feed {
    files: HashMap<String, Vec<u8>>,
}

impl ReleaseFeed for Feed {
    fn latest_url(&self) -> Result<String, String> {
        Ok(TAG_URL.into())
    }

    fn small_file(&self, url: &str, _: usize) -> Result<Vec<u8>, String> {
        self.files
            .get(url)
            .cloned()
            .ok_or_else(|| format!("missing {url}"))
    }
}

fn feed() -> Feed {
    let root = "https://github.com/lenhatthanh20/natamux/releases/download/v1.2.3/";
    Feed {
        files: HashMap::from([
            (format!("{root}SHA256SUMS.txt"), SUMS.to_vec()),
            (format!("{root}SHA256SUMS.txt.minisig"), SIGNATURE.to_vec()),
        ]),
    }
}

#[test]
fn offers_only_an_asset_named_in_the_signed_sums() {
    let outcome =
        check_now(&feed(), "1.2.2", InstallKind::Mac, "arm64", TEST_KEY).expect("signed release");
    let CheckOutcome::Asset {
        release,
        name,
        sha256,
    } = outcome
    else {
        panic!("expected a verified asset");
    };
    assert_eq!(release.tag, "v1.2.3");
    assert_eq!(name, "Natamux-mac-arm64.dmg");
    assert_eq!(
        sha256,
        "a3a807cf10fc02efe46f50cd4fd0eaa7a457d787c9d8a1bbfac1fa036e26cc36"
    );
}

#[test]
fn manual_offer_still_requires_a_valid_signature() {
    assert!(matches!(
        check_now(&feed(), "1.2.2", InstallKind::Manual, "x64", TEST_KEY),
        Ok(CheckOutcome::Manual(_))
    ));
    let mut unsigned = feed();
    unsigned.files.retain(|url, _| !url.ends_with(".minisig"));
    assert!(check_now(&unsigned, "1.2.2", InstallKind::Manual, "x64", TEST_KEY).is_err());
}

#[test]
fn rejects_the_old_release_before_downloading_files() {
    let empty = Feed {
        files: HashMap::new(),
    };
    assert!(matches!(
        check_now(&empty, "1.2.3", InstallKind::Mac, "arm64", TEST_KEY),
        Ok(CheckOutcome::UpToDate(_))
    ));
}
