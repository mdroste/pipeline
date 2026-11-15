//! A narrow caller-supplied meter. The runtime that sends a request owns its
//! cancellation and result; it receives no access to the caller's database.
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    Api,
    Subscription,
    Local,
}
pub(crate) trait Meter: Send + Sync {
    fn reserve(&self, class: Class, model: &str, input: u64, output: u64) -> Result<i64, String>;
    fn settle(&self, id: i64, model: &str, tokens: [u64; 4]) -> Result<(), String>;
}
pub(crate) type Allowance = Arc<dyn Meter>;
tokio::task_local! { pub(crate) static CURRENT: Option<Allowance>; }
pub(crate) fn current() -> Option<Allowance> {
    CURRENT.try_with(Clone::clone).ok().flatten()
}

pub(crate) fn subscription_dispatch() -> Result<(), String> {
    Hold::reserve(current().as_ref(), Class::Subscription, "", 0, 0).map(|_| ())
}

pub(crate) struct Hold(Allowance, i64);
impl Hold {
    pub fn reserve(
        meter: Option<&Allowance>,
        class: Class,
        model: &str,
        input: u64,
        output: u64,
    ) -> Result<Option<Self>, String> {
        meter
            .map(|m| {
                m.reserve(class, model, input, output)
                    .map(|id| Self(m.clone(), id))
            })
            .transpose()
    }
    pub fn settle(self, model: &str, tokens: [u64; 4]) {
        if let Err(e) = self.0.settle(self.1, model, tokens) {
            eprintln!("Could not settle usage; the reservation remains charged: {e}");
        }
    }
}
