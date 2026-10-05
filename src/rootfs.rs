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
    setup_dev()?;
    mount(
        Some("tmpfs"),
        "tmp",
        Some("tmpfs"),
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        Some("mode=1777"),
    )
    .context("mount /tmp")?;

    pivot_root(".", ".").context("pivot_root")?;
    umount2(".", MntFlags::MNT_DETACH).context("detach old root")?;
    chdir("/").context("chdir to new root")?;

    Ok(())
}

// the alpine rootfs ships an empty /dev, so bind the handful of nodes a shell expects
fn setup_dev() -> anyhow::Result<()> {
    mount(
        Some("tmpfs"),
        "dev",
        Some("tmpfs"),
        MsFlags::MS_NOSUID,
        Some("mode=755,size=256k"),
    )
    .context("mount /dev")?;

    for name in ["null", "zero", "full", "random", "urandom", "tty"] {
        let target = format!("dev/{}", name);
        std::fs::File::create(&target).with_context(|| format!("create {}", target))?;
        mount(
            Some(format!("/dev/{}", name).as_str()),
            target.as_str(),
            NONE,
            MsFlags::MS_BIND,
            NONE,
        )
        .with_context(|| format!("bind /dev/{}", name))?;
    }

    Ok(())
}
