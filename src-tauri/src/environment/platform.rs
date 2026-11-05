use super::models::{Architecture, OperatingSystem};

pub fn detect_operating_system() -> OperatingSystem {
    #[cfg(target_os = "macos")]
    {
        OperatingSystem::Macos
    }

    #[cfg(target_os = "windows")]
    {
        OperatingSystem::Windows
    }
}

pub fn detect_architecture() -> Architecture {
    #[cfg(target_arch = "aarch64")]
    {
        Architecture::Arm64
    }

    #[cfg(target_arch = "x86_64")]
    {
        Architecture::X64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_correct_operating_system() {
        let detected = detect_operating_system();

        #[cfg(target_os = "macos")]
        assert!(matches!(detected, OperatingSystem::Macos));

        #[cfg(target_os = "windows")]
        assert!(matches!(detected, OperatingSystem::Windows));
    }

    #[test]
    fn detects_correct_architecture() {
        let detected = detect_architecture();

        #[cfg(target_arch = "aarch64")]
        assert!(matches!(detected, Architecture::Arm64));

        #[cfg(target_arch = "x86_64")]
        assert!(matches!(detected, Architecture::X64));
    }
}
