//! The "Get set up" checklist: what's left to configure, worked out from what's already saved.
//! Steps tick themselves off; nothing here asks the pilot to confirm anything.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Cid,
    PushToTalk,
    Simbrief,
    Hoppie,
    Xpilot,
}

impl Step {
    /// Optional steps never hold up "ready to fly".
    pub fn optional(self) -> bool {
        matches!(self, Step::Simbrief | Step::Hoppie | Step::Xpilot)
    }
}

/// What's configured right now.
#[derive(Clone, Copy, Debug, Default)]
pub struct Progress {
    pub cid: bool,
    pub ptt_tested: bool,
    pub simbrief: bool,
    pub hoppie: bool,
    /// The xPilot plugin is installed; its step only appears then.
    pub xpilot_found: bool,
    pub xpilot_on: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    pub step: Step,
    pub done: bool,
}

/// Essentials first, then optional extras.
pub fn checklist(p: &Progress) -> Vec<Item> {
    let mut items = vec![
        Item { step: Step::Cid, done: p.cid },
        Item { step: Step::PushToTalk, done: p.ptt_tested },
        Item { step: Step::Simbrief, done: p.simbrief },
        Item { step: Step::Hoppie, done: p.hoppie },
    ];
    if p.xpilot_found {
        items.push(Item { step: Step::Xpilot, done: p.xpilot_on });
    }
    items
}

/// Every essential step is done.
pub fn ready(items: &[Item]) -> bool {
    items.iter().all(|i| i.done || i.step.optional())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xpilot_step_only_when_installed() {
        assert!(!checklist(&Progress::default()).iter().any(|i| i.step == Step::Xpilot));
        let items = checklist(&Progress { xpilot_found: true, ..Default::default() });
        assert_eq!(items.last().map(|i| i.step), Some(Step::Xpilot));
    }

    #[test]
    fn ready_needs_only_essentials() {
        assert!(!ready(&checklist(&Progress { cid: true, ..Default::default() })));
        let p = Progress { cid: true, ptt_tested: true, xpilot_found: true, ..Default::default() };
        assert!(ready(&checklist(&p)));
    }
}
