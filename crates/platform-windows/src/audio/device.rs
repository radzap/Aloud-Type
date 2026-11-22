//! Choosing which microphone to open.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

/// The saved setting: follow Windows' default, or a specific endpoint id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MicChoice {
    SystemDefault,
    Specific(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// Open the virtual default endpoint (follows Windows' default).
    Default,
    Specific(String),
    /// Chosen microphone missing: default for this Dictation, with the note.
    DefaultWithNote,
    /// No capture endpoints at all.
    NoMicrophone,
}

pub fn select(choice: &MicChoice, available: &[DeviceInfo]) -> Selection {
    if available.is_empty() {
        return Selection::NoMicrophone;
    }
    match choice {
        MicChoice::SystemDefault => Selection::Default,
        MicChoice::Specific(id) if available.iter().any(|d| &d.id == id) => {
            Selection::Specific(id.clone())
        }
        MicChoice::Specific(_) => Selection::DefaultWithNote,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dev(id: &str) -> DeviceInfo {
        DeviceInfo { id: id.into(), name: id.into() }
    }

    #[test]
    fn system_default_follows_windows() {
        assert_eq!(select(&MicChoice::SystemDefault, &[dev("a")]), Selection::Default);
    }

    #[test]
    fn present_choice_is_used() {
        assert_eq!(
            select(&MicChoice::Specific("b".into()), &[dev("a"), dev("b")]),
            Selection::Specific("b".into())
        );
    }

    #[test]
    fn missing_choice_falls_back_to_default_with_note() {
        assert_eq!(select(&MicChoice::Specific("x".into()), &[dev("a")]), Selection::DefaultWithNote);
    }

    #[test]
    fn no_endpoints_is_no_microphone_for_either_choice() {
        assert_eq!(select(&MicChoice::SystemDefault, &[]), Selection::NoMicrophone);
        assert_eq!(select(&MicChoice::Specific("x".into()), &[]), Selection::NoMicrophone);
    }
}
