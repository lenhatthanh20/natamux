#![allow(dead_code)]
//! Edge cases of Rust colours.

use std::collections::HashMap;

macro_rules! square {
    ($x:expr) => {
        $x * $x
    };
}

/// A ledger with a lifetime.
pub struct Ledger<'a, T: Clone + 'a>
where
    T: Default,
{
    names: &'a [&'a str],
    rows: HashMap<u32, T>,
}

#[derive(Debug)]
enum Entry {
    Sale { id: u32, cents: i64 },
    Refund(u32),
    Empty,
}

impl<'a, T: Clone + Default> Ledger<'a, T> {
    pub const LIMIT: usize = 1_000;

    pub async fn load(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let raw = r#"{"a": "b"}"#;
        let bytes = b"bytes\n";
        let c = '\'';
        let range = 0..=square!(3);
        for (i, name) in self.names.iter().enumerate().take(Self::LIMIT) {
            match i {
                0 => continue,
                n if n % 2 == 0 => println!("{name} {raw:?} {bytes:?} {c}"),
                _ => {}
            }
        }
        let _ = unsafe { std::mem::zeroed::<u8>() };
        let _ = range.clone().map(|x| x as f64 * 1.5e-3).collect::<Vec<_>>();
        Ok(())
    }
}

fn entry_cents(entry: &Entry) -> Option<i64> {
    if let Entry::Sale { cents, .. } = entry {
        return Some(*cents);
    }
    None
}
