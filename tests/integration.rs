use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_tartarus");

fn rootfs() -> String {
    format!("{}/rootfs/alpine", env!("CARGO_MANIFEST_DIR"))
}

fn write_config(name: &str, body: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tartarus-test-{}-{}.toml",
        name,
        std::process::id()
    ));
    fs::write(&path, body).expect("write config");
    path
}

fn config(name: &str, command: &str, memory: &str, pids: u64, blocked: &str) -> PathBuf {
    write_config(
        name,
        &format!(
            r#"rootfs = "{}"
command = {}
hostname = "testbox"

[limits]
memory = "{}"
cpus = 0.5
pids = {}

[seccomp]
blocked = [{}]
"#,
            rootfs(),
            command,
            memory,
            pids,
            blocked
        ),
    )
}

fn run(config: &Path) -> Output {
    Command::new(BIN)
        .args(["run", config.to_str().unwrap()])
        .output()
        .expect("run tartarus")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn exit_status_of_the_container_is_propagated() {
    let cfg = config(
        "exit",
        r#"["/bin/sh", "-c", "exit 7"]"#,
        "64M",
        32,
        r#""reboot""#,
    );
    assert_eq!(run(&cfg).status.code(), Some(7));
}

#[test]
fn container_is_pid_one_in_its_own_namespace() {
    let cfg = config(
        "pid",
        r#"["/bin/sh", "-c", "echo pid=$$; ps -o pid= | wc -l"]"#,
        "64M",
        32,
        r#""reboot""#,
    );
    let out = run(&cfg);
    let text = stdout(&out);
    assert!(text.contains("pid=1"), "shell was not pid 1: {text}");
    let visible: usize = text.lines().last().unwrap().trim().parse().unwrap();
    assert!(visible <= 3, "too many processes visible inside: {text}");
}

#[test]
fn rootfs_is_alpine_and_the_host_is_unreachable() {
    let cfg = config(
        "rootfs",
        r#"["/bin/sh", "-c", "head -1 /etc/os-release; ls /home | wc -l"]"#,
        "64M",
        32,
        r#""reboot""#,
    );
    let text = stdout(&run(&cfg));
    assert!(text.contains("Alpine Linux"), "not alpine: {text}");
    assert!(text.trim().ends_with('0'), "host /home leaked in: {text}");
}

#[test]
fn hostname_is_set_inside_the_uts_namespace() {
    let cfg = config("uts", r#"["/bin/hostname"]"#, "64M", 32, r#""reboot""#);
    let text = stdout(&run(&cfg));
    assert_eq!(text.trim(), "testbox");
    let host = Command::new("hostname").output().expect("host hostname");
    assert_ne!(stdout(&host).trim(), "testbox", "host hostname was changed");
}

#[test]
fn process_count_is_capped_by_pids_max() {
    let cfg = config(
        "pids",
        r#"["/bin/sh", "-c", "i=0; while [ $i -lt 100 ]; do sleep 5 & i=$((i+1)); done; echo survived"]"#,
        "64M",
        16,
        r#""reboot""#,
    );
    let out = run(&cfg);
    let text = format!("{}{}", stdout(&out), String::from_utf8_lossy(&out.stderr));
    assert!(
        text.contains("can't fork"),
        "fork was never refused: {text}"
    );
    assert!(!text.contains("survived"), "the loop completed: {text}");
}

#[test]
fn memory_limit_gets_the_container_killed() {
    let cfg = config(
        "memory",
        r#"["/bin/sh", "-c", "dd if=/dev/zero of=/tmp/x bs=1M count=500"]"#,
        "32M",
        32,
        r#""reboot""#,
    );
    // 137 is 128 + SIGKILL, which is what the out of memory killer leaves behind
    assert_eq!(run(&cfg).status.code(), Some(137));
}

#[test]
fn blocked_syscalls_fail_while_others_still_work() {
    let cfg = config(
        "seccomp",
        r#"["/bin/sh", "-c", "mkdir /tmp/blocked 2>&1; touch /tmp/allowed && echo touch-worked; grep Seccomp: /proc/self/status; grep CapEff /proc/self/status"]"#,
        "64M",
        32,
        r#""mkdir", "reboot""#,
    );
    let text = stdout(&run(&cfg));
    assert!(
        text.contains("touch-worked"),
        "touch should still work: {text}"
    );
    assert!(text.contains("Seccomp:\t2"), "no filter loaded: {text}");
    assert!(
        text.contains("CapEff:\t0000000000000000"),
        "capabilities were not dropped: {text}"
    );
}

#[test]
fn network_namespace_has_nothing_but_loopback() {
    let cfg = config(
        "net",
        r#"["/bin/sh", "-c", "ip route | wc -l; ip addr | grep -c ': '"]"#,
        "64M",
        32,
        r#""reboot""#,
    );
    let text = stdout(&run(&cfg));
    let mut lines = text.lines();
    assert_eq!(
        lines.next().unwrap().trim(),
        "0",
        "routing table is not empty: {text}"
    );
    assert_eq!(
        lines.next().unwrap().trim(),
        "1",
        "more than loopback exists: {text}"
    );
}

#[test]
fn cgroup_is_removed_after_the_container_exits() {
    // the container reports the cgroup it was placed in, so this test only looks at its own run
    let cfg = config(
        "cleanup",
        r#"["/bin/sh", "-c", "cat /proc/self/cgroup"]"#,
        "64M",
        32,
        r#""reboot""#,
    );
    let out = run(&cfg);
    assert!(out.status.success());

    let line = stdout(&out);
    let relative = line.trim().rsplit("::").next().expect("cgroup path");
    assert!(
        relative.contains("tartarus-"),
        "container was not placed in a cgroup: {line}"
    );

    let path = PathBuf::from("/sys/fs/cgroup").join(relative.trim_start_matches('/'));
    assert!(!path.exists(), "cgroup left behind: {}", path.display());
}

#[test]
fn unknown_config_keys_are_rejected() {
    let cfg = write_config(
        "badkey",
        &format!(
            r#"rootfs = "{}"
command = ["/bin/true"]
hostname = "testbox"

[limits]
memory = "64M"
cpus = 0.5
pid = 32

[seccomp]
blocked = []
"#,
            rootfs()
        ),
    );
    let out = run(&cfg);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(text.contains("unknown field"), "typo was accepted: {text}");
}

#[test]
fn empty_command_is_rejected() {
    let cfg = config("nocmd", "[]", "64M", 32, r#""reboot""#);
    let out = run(&cfg);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        text.contains("command must not be empty"),
        "unexpected error: {text}"
    );
}
