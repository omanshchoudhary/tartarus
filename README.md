# tartarus

A minimal container runtime in Rust, built directly on Linux kernel primitives: namespaces, pivot_root, cgroups v2, seccomp and capabilities.
It runs a single command inside an Alpine rootfs, isolated and resource-limited, as an unprivileged user, from a small TOML config.
Closer to runc than to Docker: no images, registries or daemons, just the isolation mechanisms themselves.

## What it does

- Starts the command in new user, mount, PID, UTS and network namespaces
- Maps your UID and GID to root inside, so no sudo is needed anywhere
- Pivots into an Alpine rootfs with a fresh `/proc`, a tmpfs `/dev` carrying the usual device nodes, and a tmpfs `/tmp`
- Applies `memory.max`, `memory.swap.max`, `cpu.max` and `pids.max` before the command starts
- Sets `no_new_privs`, drops every capability and installs a seccomp-BPF filter
- Exits with the container's own exit status, using 128 + signal number when it is killed

## Requirements

- Linux with cgroups v2 and unprivileged user namespaces (developed on Ubuntu 24.04, kernel 7.0)
- Rust 1.85 or newer (edition 2024)
- `libseccomp-dev`: `sudo apt install libseccomp-dev`
- An Alpine mini root filesystem extracted into `rootfs/alpine`

## Setup

Fetch and extract the root filesystem:

```sh
mkdir -p rootfs/alpine
curl -LO https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/x86_64/alpine-minirootfs-3.24.2-x86_64.tar.gz
tar -xzf alpine-minirootfs-3.24.2-x86_64.tar.gz -C rootfs/alpine
```

On Ubuntu 23.10 and later, AppArmor denies capabilities inside unprivileged user namespaces, so every mount fails with `EPERM` until the binary is allowed. Install the bundled profile with the correct path:

```sh
sudo cp docs/apparmor-tartarus /etc/apparmor.d/tartarus
sudo sed -i "s|/path/to/tartarus|$PWD|" /etc/apparmor.d/tartarus
sudo apparmor_parser -r /etc/apparmor.d/tartarus
```

## Usage

```sh
cargo build
./target/debug/tartarus run tartarus.toml
```

The runtime exits with the container's exit status, so it composes with shell pipelines the same way `docker run` does.

## Configuration

```toml
rootfs = "rootfs/alpine"          # extracted root filesystem
command = ["/bin/sh"]             # argv of the process to run
hostname = "tartarus"             # hostname inside the UTS namespace

[limits]
memory = "64M"                    # memory.max, with swap capped at 0
cpus = 0.5                        # share of one core, written as cpu.max
pids = 32                         # pids.max

[seccomp]
blocked = ["mount", "reboot", "unshare"]   # these syscalls return EPERM
```

Unknown keys are rejected, so a typo fails loudly instead of being ignored silently.

## Project layout

| Path | Contents |
| --- | --- |
| `src/config.rs` | TOML schema and loading |
| `src/container.rs` | Parent and child split: clone, ID mapping, synchronisation, signals, exit status |
| `src/rootfs.rs` | Mount propagation, `pivot_root`, `/proc`, `/dev` and `/tmp` |
| `src/cgroup.rs` | cgroup v2 creation, limits and cleanup |
| `src/seccomp.rs` | `no_new_privs`, capability dropping and the seccomp filter |
| `tests/integration.rs` | Integration tests, one per isolation mechanism |
| `demos/` | Ready to run configurations, one per mechanism |
| `docs/` | AppArmor profile template |

## Tests

```sh
cargo test
```

Eleven integration tests run the real binary and check what actually happened: exit status propagation, PID 1 inside its own namespace, the Alpine rootfs with the host unreachable, the hostname, the process and memory limits, blocked system calls alongside allowed ones, an empty network namespace, cgroup removal, and both config validation failures.

They need the same environment the runtime does, a systemd user session with the cpu, memory and pids controllers delegated plus the AppArmor profile installed, so CI runs formatting, clippy and the build on every push and leaves the integration tests to be run on a machine that has them.

## Demonstrations

**Isolation**

```sh
./target/debug/tartarus run demos/isolation.toml
```

Alpine userland, `ps` listing only PID 1 and `ps` itself, `uid=0(root)`, hostname `tartarus`.

**Identity**

```sh
./target/debug/tartarus run demos/identity.toml
ps -eo uid,pid,comm | grep sleep     # run in a second terminal
```

The same process reports uid 0 inside the container and uid 1000 on the host.

**Process limit**

```sh
./target/debug/tartarus run demos/forkbomb.toml
```

Process creation fails at `pids.max` with "can't fork", and the host is unaffected.

**Memory limit**

```sh
./target/debug/tartarus run demos/memory.toml; echo $?
```

The kernel kills the process when it passes `memory.max`, and the runtime exits 137.

**System call filtering**

```sh
./target/debug/tartarus run demos/seccomp.toml
```

`Seccomp: 2` and `CapEff: 0000000000000000`, with `mkdir` denied by the filter while `touch` in the same directory still works.

**Network isolation**

```sh
./target/debug/tartarus run demos/network.toml
```

Loopback is the only interface and it is down, the routing table is empty, and outbound connections fail with "Network unreachable" while the host keeps its own connectivity.

## Startup cost

The runtime is a thin layer over system calls, with no daemon and no image handling, so starting a container is close to the cost of `clone` plus `execve`. Measured on one machine, 20 runs each, running `/bin/true` in Alpine:

| | Median |
| --- | --- |
| `tartarus run` | 2.6 ms |
| `docker run alpine true` | 226 ms |

Docker is doing considerably more: a round trip to its daemon, image layer setup and network configuration. The gap is the cost of those layers, not a defect.

## How it works

The parent stays on the host and the child becomes the container, because each side can do things the other cannot.

1. Parent creates the cgroup and writes the limits
2. Parent calls `clone` with the namespace flags, and the child blocks on a pipe
3. Parent writes `uid_map`, `setgroups` and `gid_map`, adds the child to the cgroup, then releases it
4. Child sets the hostname, makes its mounts private, mounts `/proc`, `/dev` and `/tmp`, pivots into the rootfs and detaches the old root
5. Child sets `no_new_privs`, drops capabilities, loads the seccomp filter and calls `execve`
6. Parent ignores `SIGINT` and `SIGQUIT`, waits, removes the cgroup and exits with the container's status

The ordering is forced by the kernel. ID maps can only be written from outside, mounting can only be done from inside, `/proc` can only be mounted while the host's `/proc` is still visible, and the seccomp filter has to come last because it also applies to the runtime's own setup code.

## The name

In Greek mythology, Tartarus is the deepest, darkest abyss in the universe, used as a dungeon for the wicked and a prison for the defeated Titans.
A container serves the same purpose for software: code you do not trust is given a world of its own to run in, and whatever it does in there, it cannot reach the machine it was put on.
