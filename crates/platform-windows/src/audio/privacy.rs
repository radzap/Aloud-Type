//! Windows microphone privacy pre-check. Only an explicit Deny faults;
//! missing or unknown values fall through to the normal open path.

const POLICY_KEY: &str = r"Software\Policies\Microsoft\Windows\AppPrivacy";
const POLICY_VALUE: &str = "LetAppsAccessMicrophone";
const CONSENT_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
const CONSENT_NONPACKAGED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\NonPackaged";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

/// Registry read seam.
pub trait Registry {
    fn read_dword(&self, hive: Hive, key: &str, name: &str) -> Option<u32>;
    fn read_string(&self, hive: Hive, key: &str, name: &str) -> Option<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Explicit Deny: fault as Blocked without opening the stream.
    Denied,
    Proceed,
}

pub fn precheck(reg: &dyn Registry) -> Verdict {
    // Policy order: Force Deny (2), Force Allow (1), then user consent.
    match reg.read_dword(Hive::LocalMachine, POLICY_KEY, POLICY_VALUE) {
        Some(2) => return Verdict::Denied,
        Some(1) => return Verdict::Proceed,
        _ => {}
    }
    for hive in [Hive::CurrentUser, Hive::LocalMachine] {
        for key in [CONSENT_KEY, CONSENT_NONPACKAGED_KEY] {
            if let Some(v) = reg.read_string(hive, key, "Value") {
                if v.eq_ignore_ascii_case("Deny") {
                    return Verdict::Denied;
                }
            }
        }
    }
    Verdict::Proceed
}

/// The real registry.
pub struct WinRegistry;

impl WinRegistry {
    fn open(hive: Hive, key: &str) -> Option<winreg::RegKey> {
        let root = match hive {
            Hive::CurrentUser => winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER),
            Hive::LocalMachine => winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE),
        };
        root.open_subkey(key).ok()
    }
}

impl Registry for WinRegistry {
    fn read_dword(&self, hive: Hive, key: &str, name: &str) -> Option<u32> {
        Self::open(hive, key)?.get_value::<u32, _>(name).ok()
    }
    fn read_string(&self, hive: Hive, key: &str, name: &str) -> Option<String> {
        Self::open(hive, key)?.get_value::<String, _>(name).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        dwords: HashMap<(Hive, String, String), u32>,
        strings: HashMap<(Hive, String, String), String>,
    }
    impl Fake {
        fn dword(mut self, h: Hive, k: &str, n: &str, v: u32) -> Self {
            self.dwords.insert((h, k.into(), n.into()), v);
            self
        }
        fn string(mut self, h: Hive, k: &str, v: &str) -> Self {
            self.strings.insert((h, k.into(), "Value".into()), v.into());
            self
        }
    }
    impl Registry for Fake {
        fn read_dword(&self, h: Hive, k: &str, n: &str) -> Option<u32> {
            self.dwords.get(&(h, k.into(), n.into())).copied()
        }
        fn read_string(&self, h: Hive, k: &str, n: &str) -> Option<String> {
            self.strings.get(&(h, k.into(), n.into())).cloned()
        }
    }

    #[test]
    fn empty_registry_proceeds() {
        assert_eq!(precheck(&Fake::default()), Verdict::Proceed);
    }

    #[test]
    fn explicit_user_deny_is_denied() {
        let r = Fake::default().string(Hive::CurrentUser, CONSENT_KEY, "Deny");
        assert_eq!(precheck(&r), Verdict::Denied);
    }

    #[test]
    fn desktop_apps_deny_is_denied() {
        let r = Fake::default().string(Hive::CurrentUser, CONSENT_NONPACKAGED_KEY, "Deny");
        assert_eq!(precheck(&r), Verdict::Denied);
    }

    #[test]
    fn allow_and_unknown_values_proceed() {
        let r = Fake::default()
            .string(Hive::CurrentUser, CONSENT_KEY, "Allow")
            .string(Hive::CurrentUser, CONSENT_NONPACKAGED_KEY, "Prompt");
        assert_eq!(precheck(&r), Verdict::Proceed);
    }

    #[test]
    fn force_deny_policy_denies_even_when_consent_allows() {
        let r = Fake::default()
            .dword(Hive::LocalMachine, POLICY_KEY, POLICY_VALUE, 2)
            .string(Hive::CurrentUser, CONSENT_KEY, "Allow");
        assert_eq!(precheck(&r), Verdict::Denied);
    }

    #[test]
    fn force_allow_policy_overrides_user_deny() {
        let r = Fake::default()
            .dword(Hive::LocalMachine, POLICY_KEY, POLICY_VALUE, 1)
            .string(Hive::CurrentUser, CONSENT_KEY, "Deny");
        assert_eq!(precheck(&r), Verdict::Proceed);
    }

    #[test]
    fn unknown_policy_value_falls_through() {
        let r = Fake::default().dword(Hive::LocalMachine, POLICY_KEY, POLICY_VALUE, 0);
        assert_eq!(precheck(&r), Verdict::Proceed);
    }

    #[test]
    fn real_registry_does_not_panic() {
        let _ = precheck(&WinRegistry);
    }
}
