mod config;
mod container;
mod rootfs;
mod cgroup;
mod seccomp;

fn main() -> anyhow::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<String>>();

    if args.len() != 2 || args[0] != "run" {
        anyhow::bail!("usage: tartarus run <config.toml>")
    }
    let path = &args[1];
    let cfg = config::Config::load(path)?;

    let code = container::run(&cfg)?;
    std::process::exit(code);
}
