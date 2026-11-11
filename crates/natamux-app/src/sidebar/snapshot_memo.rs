//! The tree reads the dashboard into a `Snapshot` for its rows. A draw that no input changed
//! (a motion tick, a resize step, a hover) reuses the last one.

/// The inputs that have no change signal to hook, compared on each draw.
#[derive(PartialEq)]
pub(super) struct SnapshotKey {
    pub(super) window_active: bool,
    pub(super) focused: bool,
    pub(super) current: Option<String>,
    pub(super) active: Option<String>,
}

#[derive(Default)]
pub(super) struct SnapshotMemo {
    /// Nothing signalled a change since the last build.
    fresh: bool,
    key: Option<SnapshotKey>,
    #[cfg(test)]
    pub(super) builds: usize,
}

impl SnapshotMemo {
    /// The dashboard parts the rows read, or the age clock, changed.
    pub(super) fn invalidate(&mut self) {
        self.fresh = false;
    }

    pub(super) fn is_fresh(&self, key: &SnapshotKey) -> bool {
        self.fresh && self.key.as_ref() == Some(key)
    }

    pub(super) fn built(&mut self, key: SnapshotKey) {
        self.fresh = true;
        self.key = Some(key);
        #[cfg(test)]
        {
            self.builds += 1;
        }
    }
}
