#!/usr/bin/env bash
# Measures a running process over a window: CPU time, memory, and GPU memory and time.
#   spike/measure.sh <process name> [seconds]     e.g. spike/measure.sh spike 30
# Run it once for each bar, under the same load (idle, or SPIKE_AUTO=1), and compare.
#
# CPU:  utime+stime from /proc/<pid>/stat, as a % of one core.
# RAM:  RSS, and PSS (shared libraries split between their users; the fairer number).
# GPU:  the DRM fdinfo of each GPU client the process holds (i915/xe/amdgpu): memory it has, and
#       time each engine spent on it. On i915 "total"/"resident" are the sizes of the buffers the
#       driver made, not the pages behind them: a bare Vulkan device shows 216 MB here and costs
#       82 MiB. For real RAM use spike/shmem.sh.
set -eu
name=$1
secs=${2:-30}
pid=$(pgrep -x "$name" | head -1) || { echo "no process named $name" >&2; exit 1; }
hz=$(getconf CLK_TCK)

cpu() { awk '{print $14 + $15}' "/proc/$pid/stat"; }
# One line per DRM client (several fds can share one), as "key value" pairs.
gpu() {
  for f in /proc/$pid/fdinfo/*; do
    grep -q '^drm-client-id' "$f" 2>/dev/null && grep '^drm-' "$f"
  done | awk '
    # Keyed by client id, so fds that share a client count once.
    /^drm-client-id/ { id = $2; next }
    /^drm-(total|resident)-/ { mem[$1 " " id] = $2 " " $3 }
    /^drm-engine-[a-z-]+:/ && $3 == "ns" { eng[$1 " " id] = $2 }
    END {
      for (k in mem) { split(k, a, " "); split(mem[k], v, " ");
        kib = v[1] * (v[2] == "MiB" ? 1024 : v[2] == "GiB" ? 1048576 : 1); m[a[1]] += kib }
      for (k in m) printf "%s %d\n", k, m[k]
      for (k in eng) { split(k, a, " "); e[a[1]] += eng[k] }
      for (k in e) printf "%s %d\n", k, e[k]
    }'
}

c0=$(cpu); g0=$(gpu)
sleep "$secs"
c1=$(cpu); g1=$(gpu)

echo "== $name (pid $pid), ${secs}s"
awk -v d=$((c1 - c0)) -v hz="$hz" -v s="$secs" 'BEGIN { printf "CPU          %.2f%% of one core\n", d / hz / s * 100 }'
grep -E '^(VmRSS|VmHWM)' "/proc/$pid/status"
grep '^Pss:' "/proc/$pid/smaps_rollup" | awk '{ printf "Pss:\t%9d kB\n", $2 }'
# Memory: the value at the end. Engines: time used during the window, as a % of it.
join <(sort <<<"$g0") <(sort <<<"$g1") | awk -v s="$secs" '
  $1 ~ /engine/ { printf "%-28s %.2f%% busy\n", $1, ($3 - $2) / (s * 1e9) * 100; next }
  { printf "%-28s %d kB\n", $1, $3 }'
