use crate::prelude::TimestampNs;

/// Is responsible for triggering when the `AccountTracker` should sample the return of user balances.
#[derive(Debug, Clone)]
pub(crate) struct SampleReturnsTrigger {
    trigger_interval: TimestampNs,
    last_trigger: TimestampNs,
    init: bool,
}

impl SampleReturnsTrigger {
    pub(crate) fn new(trigger_interval: TimestampNs) -> Self {
        Self {
            trigger_interval,
            last_trigger: 0,
            init: true,
        }
    }

    pub(crate) fn should_trigger(&mut self, ts: TimestampNs) -> bool {
        if self.init {
            self.last_trigger = ts;
            self.init = false;
            return true;
        }

        if ts >= self.last_trigger + self.trigger_interval {
            self.last_trigger += self.trigger_interval;
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_returns_trigger() {
        let interval = 100 * 1_000_000_000;
        let mut trigger = SampleReturnsTrigger::new(interval);

        assert!(trigger.should_trigger(2 * interval));
        assert_eq!(trigger.init, false);
        assert_eq!(trigger.last_trigger, 2 * interval);
        assert!(!trigger.should_trigger(250 * 1_000_000_000));
        assert!(trigger.should_trigger(300 * 1_000_000_000));
    }
}
