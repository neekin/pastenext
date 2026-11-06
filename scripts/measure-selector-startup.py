#!/usr/bin/env python3
"""Time actual native selector first draw; captures are canceled without saving."""
import argparse
import json
from pathlib import Path
import queue
import statistics
import subprocess
import threading
import time


def sample(executable, warm):
    started = time.perf_counter()
    child = subprocess.Popen([str(executable), "--measure-startup", *( ["--warm"] if warm else ["--mode", "region"])], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    events = queue.Queue()
    def read():
        for line in child.stderr:
            events.put((time.perf_counter(), line.strip()))
    thread = threading.Thread(target=read, daemon=True)
    thread.start()
    def until(marker):
        deadline = time.perf_counter() + 15
        while time.perf_counter() < deadline:
            stamp, line = events.get(timeout=max(.01, deadline-time.perf_counter()))
            if marker in line:
                return stamp
        raise RuntimeError("Native selector did not report " + marker)
    try:
        if warm:
            until("warm-ready")
            started = time.perf_counter()
            child.stdin.write("region\tzh-CN\t0\n")
            child.stdin.flush()
        child.stdin.close()
        ready = until("first-draw")
        child.wait(timeout=10)
        result = json.loads(child.stdout.read())
        if child.returncode or not result.get("cancelled"):
            raise RuntimeError("Startup probe did not cancel cleanly")
        return round((ready-started)*1000, 2)
    finally:
        if child.poll() is None:
            child.kill()
            child.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("executable", type=Path)
    parser.add_argument("--warm", action="store_true")
    parser.add_argument("--samples", type=int, default=3)
    args = parser.parse_args()
    values = [sample(args.executable.resolve(), args.warm) for _ in range(args.samples)]
    print(json.dumps({"firstDrawMs": values, "medianMs": statistics.median(values), "warm": args.warm}))
