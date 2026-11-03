//! Keeping the Mac awake while agents work (design 5.2.13, NFR-13): an IOKit
//! assertion of type `PreventUserIdleSystemSleep`. The system releases a
//! process's assertions when the process ends, however it ends.

use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};

use crate::{Error, Result};

/// `kIOPMAssertionLevelOn` of `<IOKit/pwr_mgt/IOPMLib.h>`.
const LEVEL_ON: u32 = 255;
/// `kIOReturnSuccess`.
const SUCCESS: i32 = 0;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMAssertionCreateWithName(
        kind: CFStringRef,
        level: u32,
        name: CFStringRef,
        id: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(id: u32) -> i32;
}

/// A `PreventUserIdleSystemSleep` assertion this process holds; dropping it
/// releases it.
#[derive(Debug)]
pub struct IdleSleepAssertion(u32);

impl IdleSleepAssertion {
    /// `name` shows in `pmset -g assertions`; a constant, so no user data
    /// reaches it.
    pub fn take(name: &'static str) -> Result<Self> {
        let kind = CFString::from_static_string("PreventUserIdleSystemSleep");
        let name = CFString::from_static_string(name);
        let mut id = 0;
        // SAFETY: both strings are live CFStrings for the call, and `id` is a
        // writable `IOPMAssertionID` (a `uint32_t`).
        let status = unsafe {
            IOPMAssertionCreateWithName(
                kind.as_concrete_TypeRef(),
                LEVEL_ON,
                name.as_concrete_TypeRef(),
                &mut id,
            )
        };
        match status {
            SUCCESS => Ok(Self(id)),
            status => Err(Error::IoKit {
                call: "IOPMAssertionCreateWithName",
                status,
            }),
        }
    }
}

impl Drop for IdleSleepAssertion {
    fn drop(&mut self) {
        // SAFETY: the ID came from a successful create and is released once,
        // here. A failure leaves nothing to do: the process's end releases it.
        let _ = unsafe { IOPMAssertionRelease(self.0) };
    }
}

/// The `PreventUserIdleSystemSleep` assertions process `pid` holds, by name,
/// as `pmset -g assertions` lists them.
#[cfg(any(test, feature = "test-util"))]
pub fn idle_sleep_assertions(pid: u32) -> Vec<String> {
    let out = std::process::Command::new("/usr/bin/pmset")
        .args(["-g", "assertions"])
        .output()
        .expect("run pmset");
    let owner = format!("pid {pid}(");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| line.trim_start().starts_with(&owner))
        .filter_map(|line| {
            line.split_once(" PreventUserIdleSystemSleep named: ")?
                .1
                .split('"')
                .nth(1)
        })
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_sleep_assertion_shows_while_held() {
        let name = "cupper-sys test";
        let held = || idle_sleep_assertions(std::process::id()).contains(&name.to_owned());
        let assertion = IdleSleepAssertion::take(name).unwrap();
        assert!(held());
        drop(assertion);
        assert!(!held());
    }
}
