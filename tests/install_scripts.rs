#[cfg(unix)]
mod unix {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "rtk-codex-hook-script-{name}-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn make_release(base: &std::path::Path, log_path: &std::path::Path) {
        let payload = base.join("payload");
        std::fs::create_dir_all(&payload).expect("create payload");
        let binary = payload.join("rtk-codex-hook");
        let mut file = std::fs::File::create(&binary).expect("create fake binary");
        writeln!(
            file,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\n",
            log_path.display()
        )
        .expect("write fake binary");
        let mut permissions = std::fs::metadata(&binary)
            .expect("fake binary metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&binary, permissions).expect("chmod fake binary");

        let status = Command::new("tar")
            .arg("-czf")
            .arg(base.join("rtk-codex-hook-x86_64-unknown-linux-musl.tar.gz"))
            .arg("-C")
            .arg(&payload)
            .arg(".")
            .status()
            .expect("create archive");
        assert!(status.success(), "tar should create fake release archive");
    }

    fn run_installer(home: &std::path::Path, release_dir: &std::path::Path, path: &str) {
        let output = Command::new("sh")
            .arg("scripts/install.sh")
            .env("HOME", home)
            .env("PATH", path)
            .env("SHELL", "/bin/bash")
            .env(
                "RTK_CODEX_HOOK_DOWNLOAD_BASE_URL",
                format!("file://{}", release_dir.display()),
            )
            .output()
            .expect("run install.sh");
        assert!(
            output.status.success(),
            "installer should succeed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn unix_installer_prefers_existing_local_path_dir() {
        let root = temp_dir("existing-path");
        let home = root.join("home");
        let home_bin = home.join("bin");
        let release = root.join("release");
        let log = root.join("hook.log");
        std::fs::create_dir_all(&home_bin).expect("create home bin");
        std::fs::create_dir_all(&release).expect("create release dir");
        make_release(&release, &log);

        let path = format!("{}:/usr/bin:/bin", home_bin.display());
        run_installer(&home, &release, &path);

        assert!(home_bin.join("rtk-codex-hook").exists());
        assert!(!home.join(".local/bin/rtk-codex-hook").exists());
        assert!(!home.join(".profile").exists());
        assert_eq!(
            std::fs::read_to_string(log).expect("read hook log"),
            "--install-codex-hook\n"
        );
    }

    #[test]
    fn unix_installer_adds_managed_path_block_when_needed() {
        let root = temp_dir("path-update");
        let home = root.join("home");
        let release = root.join("release");
        let log = root.join("hook.log");
        std::fs::create_dir_all(&home).expect("create home");
        std::fs::create_dir_all(&release).expect("create release dir");
        std::fs::write(home.join(".bashrc"), "# existing bashrc\n").expect("write bashrc");
        make_release(&release, &log);

        run_installer(&home, &release, "/usr/bin:/bin");
        run_installer(&home, &release, "/usr/bin:/bin");

        let profile = std::fs::read_to_string(home.join(".bashrc")).expect("read bashrc");
        assert!(home.join(".local/bin/rtk-codex-hook").exists());
        assert!(home.join(".bashrc.bak").exists());
        assert_eq!(profile.matches("# rtk-codex-hook PATH").count(), 1);
        assert!(profile.contains(&format!(
            "export PATH=\"{}:$PATH\"",
            home.join(".local/bin").display()
        )));
        assert_eq!(
            std::fs::read_to_string(log).expect("read hook log"),
            "--install-codex-hook\n--install-codex-hook\n"
        );
    }
}

#[test]
fn powershell_installer_parses_when_pwsh_is_available() {
    let pwsh = if cfg!(windows) {
        "pwsh".to_string()
    } else if std::path::Path::new("/mnt/c/Program Files/PowerShell/7/pwsh.exe").exists() {
        "/mnt/c/Program Files/PowerShell/7/pwsh.exe".to_string()
    } else if std::path::Path::new("/mnt/c/WINDOWS/System32/WindowsPowerShell/v1.0/powershell.exe")
        .exists()
    {
        "/mnt/c/WINDOWS/System32/WindowsPowerShell/v1.0/powershell.exe".to_string()
    } else {
        eprintln!("skipping PowerShell parser test: pwsh.exe not found");
        return;
    };

    let output = std::process::Command::new(pwsh)
        .arg("-NoProfile")
        .arg("-Command")
        .arg(
            "$tokens=$null; $errors=$null; \
             [System.Management.Automation.Language.Parser]::ParseFile('scripts/install.ps1',[ref]$tokens,[ref]$errors) > $null; \
             if ($errors.Count) { $errors | ForEach-Object { Write-Error $_ }; exit 1 }",
        )
        .output()
        .expect("run PowerShell parser");
    assert!(
        output.status.success(),
        "PowerShell installer should parse\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
