#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessLaunchSpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnedProcess {
    pub program: String,
    pub pid_hint: Option<u32>,
}

pub trait ProcessSpawner {
    fn spawn(&self, spec: ProcessLaunchSpec) -> Result<SpawnedProcess, String>;
}

