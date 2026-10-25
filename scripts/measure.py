#!/usr/bin/env python3
"""Linux-only process-tree RSS/CPU sampler. Usage: python3 scripts/measure.py PID [seconds]."""
import csv
import os
from pathlib import Path
import sys
import time

pid = int(sys.argv[1])
duration = int(sys.argv[2]) if len(sys.argv) > 2 else 60
ticks = os.sysconf("SC_CLK_TCK")
page_kib = os.sysconf("SC_PAGE_SIZE") / 1024

def sample():
    records = {}
    for path in Path("/proc").glob("[0-9]*/stat"):
        try:
            # comm may contain spaces and parentheses; split after its final ')'.
            fields = path.read_text().rsplit(")", 1)[1].split()
            records[int(path.parent.name)] = (int(fields[1]), int(fields[11]) + int(fields[12]), int(fields[21]) * page_kib)
        except (OSError, ValueError, IndexError):
            continue
    tree = {pid}
    while True:
        extended = tree | {child for child, (parent, _, _) in records.items() if parent in tree}
        if extended == tree:
            break
        tree = extended
    return {p: records[p] for p in tree if p in records}

writer = csv.writer(sys.stdout)
writer.writerow(["elapsed_s", "y5_rss_kib", "tree_rss_kib", "tree_cpu_percent_one_core", "processes"])
previous = sample()
start = last = time.monotonic()
for _ in range(duration):
    time.sleep(1)
    now = time.monotonic()
    current = sample()
    if pid not in current:
        break
    delta = sum(max(0, value[1] - previous.get(p, value)[1]) for p, value in current.items())
    writer.writerow([round(now - start, 2), int(current[pid][2]), int(sum(v[2] for v in current.values())), round(delta / ticks / (now - last) * 100, 2), len(current)])
    sys.stdout.flush()
    previous, last = current, now
