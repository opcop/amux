#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslPathMapping {
    pub distro: String,
    pub unix_path: String,
    pub unc_path: String,
}

pub fn wsl_unc_path(distro: &str, unix_path: &str) -> String {
    let normalized = unix_path.trim_start_matches('/').replace('/', "\\");
    format!(r"\\wsl$\{}\{}", distro, normalized)
}

#[cfg(test)]
mod tests {
    use super::wsl_unc_path;

    #[test]
    fn converts_unix_path_to_unc_path() {
        assert_eq!(
            wsl_unc_path("Ubuntu", "/home/user/amux"),
            r"\\wsl$\Ubuntu\home\user\amux"
        );
    }
}
