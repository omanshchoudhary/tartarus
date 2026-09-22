use crate::config::Config;
use nix::sched::{CloneFlags, clone};
use nix::sys::signal::{SigHandler, Signal, signal};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{getgid, getuid, sethostname};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};

// 1 MB stack allocation for cloned child process
const STACK_SIZE: usize = 1024 * 1024;

pub fn run(cfg: &Config) -> anyhow::Result<i32> {
    // pipe for child and parent synchronization
    let (mut reader, mut writer) = std::io::pipe()?;

    // allocate scratch memory for the child process stack
    let mut stack = vec![0u8; STACK_SIZE];

    let cfg_ref = cfg;

    // wrapping child execution
    let child_fn = Box::new(move || match child_main(cfg_ref, &mut reader) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("[Child Error] {}", err);
            1
        }
    });

    // spawn child
    let clone_flags =
        CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWUTS | CloneFlags::CLONE_NEWPID;

    let pid = unsafe {
        clone(
            child_fn,
            &mut stack,
            clone_flags,
            Some(Signal::SIGCHLD as i32),
        )?
    };

    // parent process execution block
    let child_pid = pid.as_raw();

    unsafe {
        signal(Signal::SIGINT, SigHandler::SigIgn)?;
        signal(Signal::SIGQUIT, SigHandler::SigIgn)?;
    }

    let uid = getuid();
    let gid = getgid();

    // map parent UID to UID 0 (root) inside the container namespace
    let mut uid_map_file = File::create(format!("/proc/{}/uid_map", child_pid))?;
    uid_map_file.write_all(format!("0 {} 1\n", uid).as_bytes())?;

    // disable extra groups to fulfill security constraints
    let mut setgroups_file = File::create(format!("/proc/{}/setgroups", child_pid))?;
    setgroups_file.write_all(b"deny\n")?;

    // map parent GID to GID 0 inside the container namespace
    let mut gid_map_file = File::create(format!("/proc/{}/gid_map", child_pid))?;
    gid_map_file.write_all(format!("0 {} 1\n", gid).as_bytes())?;

    // send "go" signal byte to unblock child
    writer.write_all(b"g")?;
    drop(writer);

    // wait for the child process to complete execution
    let exit_code = match waitpid(pid, None)? {
        WaitStatus::Exited(_, code) => code,
        WaitStatus::Signaled(_, sig, _) => 128 + (sig as i32),
        _ => 1
    };

    Ok(exit_code)
}

// waits for parent signal and then attempt to set the hostname, prints namespace context
// this is what the container is doing
pub fn child_main(cfg: &Config, reader: &mut std::io::PipeReader) -> anyhow::Result<()> {
    // waiting for parent signal
    let mut sync_buf = [0u8; 1];
    reader.read_exact(&mut sync_buf)?;

    // set system hostname inside UTS namespace
    sethostname(&cfg.hostname)?;

    if cfg.command.is_empty() {
        anyhow::bail!("config: command must not be empty");
    }

    let args: Vec<CString> = cfg
        .command
        .iter()
        .map(|s| CString::new(s.as_str()))
        .collect::<Result<Vec<CString>, _>>()?;

    let env_strings = vec![
        "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        "HOME=/root",
        "TERM=xterm-256color",
    ];

    let env = env_strings
        .into_iter()
        .map(|e| CString::new(e))
        .collect::<Result<Vec<CString>, _>>()?;

    nix::unistd::execve(&args[0], &args, &env)?;
    Ok(())
}
