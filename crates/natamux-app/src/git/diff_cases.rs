//! The texts the diff tests share: the 6 shapes of the T-0230 bugs with every final-break
//! combination, word-diff fixtures in JS, CSS and JSON, and 20 seeded random edits (T-DIFF-1).

pub fn numbered(word: &str, count: u32) -> String {
    (0..count).map(|line| format!("{word} {line}\n")).collect()
}

/// The text without its final line break, when `final_break` is false.
pub fn ended(text: &str, final_break: bool) -> String {
    if final_break {
        text.to_owned()
    } else {
        text.trim_end_matches('\n').to_owned()
    }
}

/// The shape of the bug: 3 lines changed, then 147 lines deleted at the end of the file.
pub fn changed_then_deleted() -> (String, String) {
    let original = numbered("kept", 277) + &numbered("old", 3) + &numbered("gone", 147);
    let modified = numbered("kept", 277) + &numbered("new", 3);
    (original, modified)
}

pub fn deleted_at_end() -> (String, String) {
    let original = numbered("kept", 280) + &numbered("gone", 147);
    let modified = numbered("kept", 280);
    (original, modified)
}

/// Hunks in the middle (changed, added, deleted) and lines deleted at the end.
pub fn several_hunks() -> (String, String) {
    let original = numbered("a", 50)
        + &numbered("old", 5)
        + &numbered("b", 50)
        + &numbered("c", 50)
        + &numbered("gone", 20)
        + &numbered("d", 30)
        + &numbered("tail", 100);
    let modified = numbered("a", 50)
        + &numbered("new", 2)
        + &numbered("b", 50)
        + &numbered("added", 10)
        + &numbered("c", 50)
        + &numbered("d", 30);
    (original, modified)
}

pub fn swapped((original, modified): (String, String)) -> (String, String) {
    (modified, original)
}

/// All shapes with a final line break in both texts, in neither, and in only one; with a name
/// for the failure message.
pub fn cases() -> Vec<(String, (String, String))> {
    let shapes = [
        ("changed then deleted", changed_then_deleted()),
        ("deleted at end", deleted_at_end()),
        ("changed then added", swapped(changed_then_deleted())),
        ("added at end", swapped(deleted_at_end())),
        ("several hunks", several_hunks()),
        ("several hunks added", swapped(several_hunks())),
    ];
    let mut cases = Vec::new();
    for (name, (original, modified)) in shapes {
        for (old_break, new_break) in [(true, true), (false, false), (true, false), (false, true)] {
            cases.push((
                format!("{name}, final breaks {old_break} {new_break}"),
                (ended(&original, old_break), ended(&modified, new_break)),
            ));
        }
    }
    cases
}

/// `(name, path, old, new)`: changes with `$ # - . :` inside words and runs such as `==`, `::`
/// and `=>`, in the languages whose word characters differ.
pub fn word_fixtures() -> Vec<(String, &'static str, (String, String))> {
    let pair = |name: &str, path: &'static str, old: &str, new: &str| {
        (
            name.to_owned(),
            path,
            (format!("{old}\n"), format!("{new}\n")),
        )
    };
    vec![
        pair(
            "js dollar",
            "words.js",
            "const $el = $(\"#a-b\");",
            "const $el2 = $(\"#a-c\");",
        ),
        pair(
            "js equals",
            "words.js",
            "if (a == b && c.d != e) {",
            "if (a === b && c.e !== e) {",
        ),
        pair(
            "js arrow",
            "words.js",
            "const f = (x) => x + 1;",
            "const f = (x) => { return x + 1; };",
        ),
        pair(
            "js dotted",
            "words.js",
            "const total = price.base-cost.x;",
            "const total = price.base-cost.y;",
        ),
        pair(
            "js label",
            "words.js",
            "const o = { key:value, other::z };",
            "const o = { key: value, other::y };",
        ),
        pair(
            "js two lines",
            "words.js",
            "let a_b = 1;\nlet c-d = 2;",
            "let a_c = 1;\nlet c-e = 3;",
        ),
        pair(
            "css pseudo",
            "words.css",
            "a::before { color: red; }",
            "a::after { color: blue; }",
        ),
        pair(
            "css class",
            "words.css",
            ".foo-bar:hover { margin-top: 1px; }",
            ".foo-baz:focus { margin-top: 2px; }",
        ),
        pair(
            "css id",
            "words.css",
            "#main-nav .item { --gap-size: 4px; }",
            "#main-bar .item { --gap-width: 4px; }",
        ),
        pair(
            "css dollar",
            "words.css",
            "$base-color: #fff;",
            "$base-colour: #eee;",
        ),
        pair(
            "json key",
            "words.json",
            "  \"my-key\": \"v.1\",",
            "  \"my-key2\": \"v.2\",",
        ),
        pair(
            "json colon",
            "words.json",
            "  \"a:b\": [1, 2, 3],",
            "  \"a:c\": [1, 2, 4],",
        ),
        pair(
            "json arrow",
            "words.json",
            "  \"link\": \"a=>b\", \"x\": \"==\"",
            "  \"link\": \"a=>c\", \"x\": \"::\"",
        ),
    ]
}

/// A small generator, so the random cases are the same on every machine.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn below(&mut self, count: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % count as u64) as usize
    }
}

const POOL: [&str; 10] = [
    "alpha", "beta-two", "gamma.x", "$delta", "#eps", "a::b", "==", "=>", "x_y", "foo(bar)",
];

fn random_line(rng: &mut Rng, index: usize) -> String {
    let words: Vec<&str> = (0..2 + rng.below(4)).map(|_| POOL[rng.below(10)]).collect();
    format!("line{index} {}", words.join(" "))
}

/// 20 edits of a 40 line text: a few changed, deleted and inserted runs of lines.
pub fn random_cases() -> Vec<(String, &'static str, (String, String))> {
    let paths = ["random.js", "random.css", "random.json"];
    (0..20u64)
        .map(|seed| {
            let mut rng = Rng::new(seed + 1);
            let old: Vec<String> = (0..40).map(|i| random_line(&mut rng, i)).collect();
            let mut new = old.clone();
            for _ in 0..1 + rng.below(5) {
                let at = rng.below(new.len().max(1));
                match rng.below(3) {
                    0 => {
                        let line = random_line(&mut rng, 100 + at);
                        if let Some(slot) = new.get_mut(at) {
                            *slot = line;
                        }
                    }
                    1 => {
                        let count = (1 + rng.below(4)).min(new.len() - at);
                        new.drain(at..at + count);
                    }
                    _ => {
                        for k in 0..1 + rng.below(4) {
                            let line = random_line(&mut rng, 200 + at + k);
                            new.insert(at, line);
                        }
                    }
                }
            }
            let end = |lines: &[String], final_break: bool| {
                let text = lines.join("\n");
                if final_break { text + "\n" } else { text }
            };
            let texts = (end(&old, true), end(&new, seed % 3 != 0));
            (format!("random {seed}"), paths[seed as usize % 3], texts)
        })
        .collect()
}
