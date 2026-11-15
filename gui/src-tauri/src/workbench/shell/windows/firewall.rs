//! Per-AppContainer block-only WFP rules. Transactions publish the entire policy
//! before the workload gate opens. Rules survive guard failure and BFE restart;
//! cleanup first proves every process with that identity has stopped. Ownership
//! metadata in BFE permits a later guard to reap abandoned rules safely.
use super::identity::{self, Handle, Sid};
use serde::{Deserialize, Serialize};
use std::ptr::{null, null_mut};
use windows_sys::{
    core::GUID,
    Win32::{Foundation::*, NetworkManagement::WindowsFilteringPlatform::*},
};

const PROVIDER: GUID = GUID::from_u128(0x43cc567c_2b72_4603_a91d_9b5bac76d317);
const SUBLAYER: GUID = GUID::from_u128(0x6472a221_a324_4c36_b2a1_91863e9c7c20);
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    version: u8,
    guard: u32,
    created: u64,
    package: String,
    user: String,
    id: String,
}
pub(super) struct Rules {
    owner: Owner,
    filters: Vec<u64>,
}
pub(super) struct Engine(HANDLE);
impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            FwpmEngineClose0(self.0);
        }
    }
}
fn check(code: u32) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("Windows command firewall error {code:#x}"))
    }
}
pub(super) fn nonce() -> Result<String, String> {
    let mut bytes = [0; 24];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
impl Engine {
    pub fn open() -> Result<Self, String> {
        let mut raw = null_mut();
        check(unsafe { FwpmEngineOpen0(null(), 10, null(), null(), &mut raw) })?;
        let engine = Self(raw);
        let mut name = identity::wide("Pipeline command restrictions");
        let provider = FWPM_PROVIDER0 {
            providerKey: PROVIDER,
            flags: FWPM_PROVIDER_FLAG_PERSISTENT,
            displayData: FWPM_DISPLAY_DATA0 {
                name: name.as_mut_ptr(),
                description: null_mut(),
            },
            ..Default::default()
        };
        let code = unsafe { FwpmProviderAdd0(raw, &provider, null_mut()) };
        if code != FWP_E_ALREADY_EXISTS as u32 {
            check(code)?;
        }
        let layer = FWPM_SUBLAYER0 {
            subLayerKey: SUBLAYER,
            providerKey: &PROVIDER as *const _ as *mut _,
            flags: FWPM_SUBLAYER_FLAG_PERSISTENT,
            weight: u16::MAX,
            displayData: provider.displayData,
            ..Default::default()
        };
        let code = unsafe { FwpmSubLayerAdd0(raw, &layer, null_mut()) };
        if code != FWP_E_ALREADY_EXISTS as u32 {
            check(code)?;
        }
        engine.reap()?;
        Ok(engine)
    }
    fn transaction<T>(&self, body: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        check(unsafe { FwpmTransactionBegin0(self.0, 0) })?;
        let result = body().and_then(|value| {
            check(unsafe { FwpmTransactionCommit0(self.0) })?;
            Ok(value)
        });
        if result.is_err() {
            unsafe {
                FwpmTransactionAbort0(self.0);
            }
        }
        result
    }
    pub fn restrict(&self, child: &Handle, port: u16, user: &str) -> Result<Rules, String> {
        if port == 0 || identity::user(child)? != user {
            return Err("Invalid command firewall target".into());
        }
        let package = identity::package(child)?.ok_or("Command has no AppContainer identity")?;
        if !package.starts_with("S-1-15-2-") {
            return Err("Invalid command AppContainer identity".into());
        }
        let owner = Owner {
            version: 1,
            guard: std::process::id(),
            created: identity::created(&identity::process(std::process::id(), false)?)?,
            package,
            user: user.into(),
            id: nonce()?,
        };
        let sid = Sid::parse(&owner.package)?;
        let mut metadata = serde_json::to_vec(&owner).map_err(|e| e.to_string())?;
        let mut name = identity::wide("Pipeline command restrictions");
        let package = FWPM_FILTER_CONDITION0 {
            fieldKey: FWPM_CONDITION_ALE_PACKAGE_ID,
            matchType: FWP_MATCH_EQUAL,
            conditionValue: FWP_CONDITION_VALUE0 {
                r#type: FWP_SID,
                Anonymous: FWP_CONDITION_VALUE0_0 { sid: sid.0.cast() },
            },
        };
        let filters = self.transaction(|| {
            let mut filters = Vec::new();
            let mut add = |layer, extra: Option<FWPM_FILTER_CONDITION0>| -> Result<(), String> {
                let mut conditions = vec![package];
                if let Some(extra) = extra {
                    conditions.push(extra);
                }
                let filter = FWPM_FILTER0 {
                    displayData: FWPM_DISPLAY_DATA0 {
                        name: name.as_mut_ptr(),
                        description: null_mut(),
                    },
                    flags: FWPM_FILTER_FLAG_PERSISTENT,
                    providerKey: &PROVIDER as *const _ as *mut _,
                    providerData: FWP_BYTE_BLOB {
                        size: metadata.len() as u32,
                        data: metadata.as_mut_ptr(),
                    },
                    layerKey: layer,
                    subLayerKey: SUBLAYER,
                    numFilterConditions: conditions.len() as u32,
                    filterCondition: conditions.as_mut_ptr(),
                    action: FWPM_ACTION0 {
                        r#type: FWP_ACTION_BLOCK,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut id = 0;
                check(unsafe { FwpmFilterAdd0(self.0, &filter, null_mut(), &mut id) })?;
                filters.push(id);
                Ok(())
            };
            for layer in [
                FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,
                FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6,
                FWPM_LAYER_ALE_AUTH_CONNECT_V6,
            ] {
                add(layer, None)?;
            }
            for extra in [
                FWPM_FILTER_CONDITION0 {
                    fieldKey: FWPM_CONDITION_IP_REMOTE_ADDRESS,
                    matchType: FWP_MATCH_NOT_EQUAL,
                    conditionValue: FWP_CONDITION_VALUE0 {
                        r#type: FWP_UINT32,
                        Anonymous: FWP_CONDITION_VALUE0_0 { uint32: 0x7f000001 },
                    },
                },
                FWPM_FILTER_CONDITION0 {
                    fieldKey: FWPM_CONDITION_IP_REMOTE_PORT,
                    matchType: FWP_MATCH_NOT_EQUAL,
                    conditionValue: FWP_CONDITION_VALUE0 {
                        r#type: FWP_UINT16,
                        Anonymous: FWP_CONDITION_VALUE0_0 { uint16: port },
                    },
                },
                FWPM_FILTER_CONDITION0 {
                    fieldKey: FWPM_CONDITION_IP_PROTOCOL,
                    matchType: FWP_MATCH_NOT_EQUAL,
                    conditionValue: FWP_CONDITION_VALUE0 {
                        r#type: FWP_UINT8,
                        Anonymous: FWP_CONDITION_VALUE0_0 { uint8: 6 },
                    },
                },
            ] {
                add(FWPM_LAYER_ALE_AUTH_CONNECT_V4, Some(extra))?;
            }
            Ok(filters)
        })?;
        Ok(Rules { owner, filters })
    }
    pub fn finish(&self, rules: &Rules) -> Result<(), String> {
        identity::terminate_identity(&rules.owner.package, &rules.owner.user)?;
        self.transaction(|| {
            for id in &rules.filters {
                check(unsafe { FwpmFilterDeleteById0(self.0, *id) })?;
            }
            Ok(())
        })
    }
    fn reap(&self) -> Result<(), String> {
        let mut enumeration = null_mut();
        // A null template enumerates all layers. A zero layer GUID inside a
        // nonnull template is an invalid layer, not a wildcard.
        check(unsafe { FwpmFilterCreateEnumHandle0(self.0, null(), &mut enumeration) })?;
        let result = self.reap_entries(enumeration);
        unsafe {
            FwpmFilterDestroyEnumHandle0(self.0, enumeration);
        }
        result
    }
    fn reap_entries(&self, enumeration: HANDLE) -> Result<(), String> {
        let mut groups = std::collections::BTreeMap::<String, Rules>::new();
        loop {
            let mut entries = null_mut();
            let mut count = 0;
            check(unsafe { FwpmFilterEnum0(self.0, enumeration, 128, &mut entries, &mut count) })?;
            for i in 0..count as usize {
                let filter = unsafe { &**entries.add(i) };
                let Some(provider) = (unsafe { filter.providerKey.as_ref() }) else {
                    continue;
                };
                if provider.data1 != PROVIDER.data1
                    || provider.data2 != PROVIDER.data2
                    || provider.data3 != PROVIDER.data3
                    || provider.data4 != PROVIDER.data4
                {
                    continue;
                }
                if filter.providerData.size == 0
                    || filter.providerData.size > 2048
                    || filter.providerData.data.is_null()
                {
                    continue;
                }
                let bytes = unsafe {
                    std::slice::from_raw_parts(
                        filter.providerData.data,
                        filter.providerData.size as usize,
                    )
                };
                let Ok(owner) = serde_json::from_slice::<Owner>(bytes) else {
                    continue;
                };
                if owner.version != 1 || !owner.package.starts_with("S-1-15-2-") {
                    continue;
                }
                // Full metadata is the grouping key; malformed or future entries
                // remain restrictive instead of authorizing partial cleanup.
                let key = String::from_utf8_lossy(bytes).into_owned();
                groups
                    .entry(key)
                    .or_insert(Rules {
                        owner,
                        filters: Vec::new(),
                    })
                    .filters
                    .push(filter.filterId);
            }
            if !entries.is_null() {
                unsafe {
                    FwpmFreeMemory0((&mut entries as *mut *mut *mut FWPM_FILTER0).cast());
                }
            }
            if count < 128 {
                break;
            }
        }
        for rules in groups.values() {
            let abandoned = match identity::process(rules.owner.guard, false) {
                Ok(process) => {
                    !process.alive() || identity::created(&process)? != rules.owner.created
                }
                Err(_) => (unsafe { GetLastError() }) == ERROR_INVALID_PARAMETER,
            };
            if abandoned {
                self.finish(rules)?;
            }
        }
        Ok(())
    }
}
impl Rules {
    pub fn id(&self) -> &str {
        &self.owner.id
    }
    pub fn package(&self) -> &str {
        &self.owner.package
    }
}
