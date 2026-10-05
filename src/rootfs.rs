use anyhow::Context;
use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::unistd::{chdir, pivot_root};

const NONE: Option<&str> = None;

pub fn setup(rootfs: &str) -> anyhow::Result<()> {
    let root = std::fs::canonicalize(rootfs)
        .with_context(|| format!("rootfs not found: {}", rootfs))?;

    // avoid private mounts propagate to host
    mount(NONE, "/", NONE, MsFlags::MS_REC | MsFlags::MS_PRIVATE, NONE).context("make / private")?;

    mount(
        Some(&root),
        &root,
        NONE,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        NONE,
    )
    .context("bind mount rootfs")?;
    chdir(&root).context("chdir to rootfs")?;

    mount(Some("proc"), "proc", Some("proc"), MsFlags::empty(), NONE).context("mount proc")?;
    pivot_root(".", ".").context("pivot_root")?;
    umount2(".", MntFlags::MNT_DETACH).context("detach old root")?;
    chdir("/").context("chdir to new root")?;

    Ok(())
}
