use anyhow::Context;
use caps::CapSet;
use libseccomp::{ScmpAction, ScmpFilterContext, ScmpSyscall};
use nix::libc::EPERM;
use nix::sys::prctl;

pub fn apply(blocked: &[String]) -> anyhow::Result<()> {
    // Prevent the container process from gaining additional privileges.
    prctl::set_no_new_privs().context("set no_new_privs")?;

    let cap_sets_in_order = [
        CapSet::Ambient,
        CapSet::Bounding,
        CapSet::Inheritable,
        CapSet::Effective,
        CapSet::Permitted,
    ];

    for set in cap_sets_in_order {
        // Clear all capabilities for the given set
        caps::clear(None, set).with_context(|| format!("clear {:?} caps", set))?;
    }
    let mut filter = ScmpFilterContext::new(ScmpAction::Allow)?;

    for name in blocked {
        let syscall = ScmpSyscall::from_name(name)
            .with_context(|| format!("unknown syscall in config: {}", name))?;
        // Block system call with EPERM error
        filter.add_rule(ScmpAction::Errno(EPERM), syscall)?;
    }
    filter.load()?;

    Ok(())
}
