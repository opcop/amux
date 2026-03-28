#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchProfile {
    pub name: String,
    pub env: Vec<(String, String)>,
}

