#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplyName {
    Create,
    Connect,
    ProcessInput,
    HeartBeat,
    GetPreload,
    CleanUp,
}

impl ApplyName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Connect => "connect",
            Self::ProcessInput => "process_input",
            Self::HeartBeat => "heart_beat",
            Self::GetPreload => "get_preload",
            Self::CleanUp => "clean_up",
        }
    }
}

impl AsRef<str> for ApplyName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
