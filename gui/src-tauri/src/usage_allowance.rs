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

/// Native API agent loops cannot be bounded using the direct-API token estimator.
/// Mark them unpriced, so a caller's dollar cap fails closed before dispatch.
pub(crate) const CLAUDE_CODE_API_BILLING: &str = "Claude Code API (native billing)";

pub(crate) fn claude_dispatch(mode: crate::agent_runtime::claude::AuthMode) -> Result<(), String> {
    let class = if mode == crate::agent_runtime::claude::AuthMode::Api {
        Class::Api
    } else {
        Class::Subscription
    };
    Hold::reserve(current().as_ref(), class, CLAUDE_CODE_API_BILLING, 0, 0).map(|_| ())
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

#[cfg(test)]
mod tests {
    use super::*;
    struct CappedMeter;
    impl Meter for CappedMeter {
        fn reserve(
            &self,
            class: Class,
            model: &str,
            input: u64,
            output: u64,
        ) -> Result<i64, String> {
            if class == Class::Api
                && crate::output::estimate_cost(model, input, output, 0, 0).is_none()
            {
                return Err("API cost cannot be bounded".into());
            }
            Ok(1)
        }
        fn settle(&self, _: i64, _: &str, _: [u64; 4]) -> Result<(), String> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn claude_native_api_is_never_counted_as_subscription_quota() {
        use crate::agent_runtime::claude::AuthMode;
        assert!(crate::output::estimate_cost(CLAUDE_CODE_API_BILLING, 0, 0, 0, 0).is_none());
        CURRENT
            .scope(Some(Arc::new(CappedMeter) as Allowance), async {
                assert!(claude_dispatch(AuthMode::Subscription).is_ok());
                assert_eq!(
                    claude_dispatch(AuthMode::Api).unwrap_err(),
                    "API cost cannot be bounded"
                );
            })
            .await;
    }
}
