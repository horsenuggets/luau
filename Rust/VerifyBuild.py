import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


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


def verify_git_source(environment, target):
    repository = Path(__file__).resolve().parents[1]
    revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=repository, text=True,
    ).strip()
    url = repository.as_uri()
    consumer = repository / "Rust" / "Consumer"
    manifest = (consumer / "Cargo.toml").read_text()
    manifest += (
        "\n[lib]\npath = " + json.dumps(str(consumer / "src" / "lib.rs"))
        + "\n\n[patch.crates-io]\nluau0-src = { git = " + json.dumps(url)
        + ", rev = " + json.dumps(revision) + " }\n"
    )
    with tempfile.TemporaryDirectory(prefix="luau-git-consumer-") as directory:
        project = Path(directory)
        (project / "Cargo.toml").write_text(manifest)
        environment = environment.copy()
        environment["CARGO_HOME"] = str(project / "cargo-home")
        environment["CARGO_TARGET_DIR"] = str(project / "target")
        environment["GIT_TERMINAL_PROMPT"] = "0"
        subprocess.run(
            ["cargo", "generate-lockfile"], cwd=project, env=environment, check=True,
        )
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--locked", "--format-version=1"],
            cwd=project, env=environment, text=True,
        ))
        sources = [
            package for package in metadata["packages"]
            if package["name"] == "luau0-src"
        ]
        assert len(sources) == 1, "Expected exactly one native source package."
        assert sources[0]["source"] == f"git+{url}?rev={revision}#{revision}"
        command = ["cargo", "test", "--locked", "--features", "codegen"]
        if target:
            command.extend(["--target", target])
        subprocess.run(command, cwd=project, env=environment, check=True)
    print("Verified pinned Git source resolution and native consumer execution.")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--libcxx", choices=["CXXFLAGS", "LUAU_CXXFLAGS"])
    mode.add_argument("--git-source", action="store_true")
    options = parser.parse_args()
    environment = os.environ.copy()
    if options.git_source:
        verify_git_source(environment, options.target)
        return
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
