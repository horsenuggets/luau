import argparse
import json
import os
from pathlib import Path
import subprocess


def run_consumer(environment, target):
    command = [
        "cargo", "test", "--locked", "-p", "luau-source-consumer",
        "--message-format=json",
    ]
    if target:
        command.extend(["--target", target])
    result = subprocess.run(command, env=environment, text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    messages = []
    for line in result.stdout.splitlines():
        if line.startswith("{"):
            messages.append(json.loads(line))
    artifact = next(
        message for message in messages
        if message.get("reason") == "compiler-artifact"
        and message["target"]["name"] == "mlua_sys"
    )
    build = next(
        message for message in messages
        if message.get("reason") == "build-script-executed"
        and "mlua-sys@" in message["package_id"]
    )
    output = (Path(build["out_dir"]).parent / "output").read_text()
    assert "cargo:rerun-if-env-changed=CXXFLAGS\n" in output
    assert "cargo:rerun-if-env-changed=AR\n" in output
    return artifact, build


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target")
    parser.add_argument("--libcxx", choices=["CXXFLAGS", "LUAU_CXXFLAGS"])
    options = parser.parse_args()
    environment = os.environ.copy()
    if options.libcxx:
        environment["CXX"] = "clang++"
        environment[options.libcxx] = (
            environment.get(options.libcxx, "") + " -stdlib=libc++"
        )
    flags = environment.get("CXXFLAGS", "")
    environment["CXXFLAGS"] = flags + " -DLUAU_BRIDGE_REBUILD_PROBE=1"
    run_consumer(environment, options.target)
    artifact, _ = run_consumer(environment, options.target)
    assert artifact["fresh"], "An unchanged build must reuse its native archives."
    environment["CXXFLAGS"] = flags + " -DLUAU_BRIDGE_REBUILD_PROBE=2"
    artifact, build = run_consumer(environment, options.target)
    assert not artifact["fresh"], "Changed compiler flags must rebuild native archives."
    if options.libcxx:
        assert "c++" in build["linked_libs"], build["linked_libs"]
        assert "stdc++" not in build["linked_libs"], build["linked_libs"]
    print("Verified compiler tracking, archive reuse and native runtime linking.")


if __name__ == "__main__":
    main()
