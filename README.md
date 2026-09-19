# tartarus

A minimal container runtime in Rust, built directly on Linux kernel primitives: namespaces, pivot_root, cgroups v2, seccomp and capabilities.
It runs a single command inside an Alpine rootfs, isolated and resource-limited, as an unprivileged user, from a small TOML config.
Closer to runc than to Docker: no images, registries or daemons, just the isolation mechanisms themselves.
