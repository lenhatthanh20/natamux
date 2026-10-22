//! The toast queue (`lib/toasts.ts`). The view keeps what an action runs, by the index of the
//! action in the toast.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastTone {
    Info,
    Success,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    pub text: String,
    pub tone: ToastTone,
    pub detail: Option<String>,
    /// The labels of the buttons.
    pub actions: Vec<String>,
}

const MAX_TOASTS: usize = 3;

#[derive(Debug, Default)]
pub struct Toasts {
    pub toasts: Vec<Toast>,
    next_id: u64,
}

impl Toasts {
    /// The same message twice in a row shows once: the new one replaces the old. At most three
    /// stay, the newest last.
    pub fn push(
        &mut self,
        text: &str,
        tone: ToastTone,
        detail: Option<String>,
        actions: Vec<String>,
    ) {
        self.next_id += 1;
        self.toasts.retain(|toast| toast.text != text);
        self.toasts.push(Toast {
            id: self.next_id,
            text: text.into(),
            tone,
            detail,
            actions,
        });
        let extra = self.toasts.len().saturating_sub(MAX_TOASTS);
        self.toasts.drain(..extra);
    }

    pub fn dismiss(&mut self, id: u64) {
        self.toasts.retain(|toast| toast.id != id);
    }
}

/// How long a toast stays, in ms. None: until the user dismisses it.
pub fn toast_duration(tone: ToastTone, has_actions: bool) -> Option<u32> {
    match tone {
        ToastTone::Error if has_actions => None,
        ToastTone::Error => Some(10_000),
        _ => Some(5000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_info_for_5s_errors_for_10s_and_errors_with_an_action_until_dismissed() {
        assert_eq!(toast_duration(ToastTone::Info, false), Some(5000));
        assert_eq!(toast_duration(ToastTone::Success, false), Some(5000));
        assert_eq!(toast_duration(ToastTone::Error, false), Some(10_000));
        assert_eq!(toast_duration(ToastTone::Error, true), None);
    }

    #[test]
    fn shows_at_most_3_and_drops_a_repeated_message() {
        let mut toasts = Toasts::default();
        for text in ["a", "b", "c", "d", "d"] {
            toasts.push(text, ToastTone::Info, None, Vec::new());
        }
        let texts: Vec<&str> = toasts.toasts.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["b", "c", "d"]);
    }

    #[test]
    fn gives_each_toast_its_own_id_and_dismisses_by_id() {
        let mut toasts = Toasts::default();
        toasts.push("a", ToastTone::Info, None, Vec::new());
        toasts.push(
            "b",
            ToastTone::Error,
            Some("why".into()),
            vec!["Retry".into()],
        );
        let first = toasts.toasts[0].id;
        assert_ne!(first, toasts.toasts[1].id);
        toasts.dismiss(first);
        assert_eq!(toasts.toasts.len(), 1);
        assert_eq!(toasts.toasts[0].text, "b");
    }
}
