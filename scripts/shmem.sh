#!/usr/bin/env bash
# Real RAM a GPU process costs: Shmem and MemAvailable before and while it runs, several times.
# i915 keeps GPU buffers in shmem, and wl_shm buffers are memfds, so Shmem catches both bars'.
# MemAvailable is too noisy to read alone; it is there to catch a gross difference.
# Usage: scripts/shmem.sh <worktree> <binary path> [env...]
#   scripts/shmem.sh "$PWD" ./target/release/my_shell
cd "$1"; bin=$2; shift 2
name=$(basename "$bin")
m() { awk -v k="$1" '$1 == k":" { print $2 }' /proc/meminfo; }
for i in 1 2 3 4 5; do
  s0=$(m Shmem); a0=$(m MemAvailable)
  env "$@" timeout 16 nix develop --command "$bin" >/dev/null 2>&1 &
  sleep 10
  s1=$(m Shmem); a1=$(m MemAvailable)
  echo "run $i: Shmem +$(( (s1 - s0) / 1024 )) MiB, MemAvailable -$(( (a0 - a1) / 1024 )) MiB"
  pkill -x "$name"; sleep 3
done
