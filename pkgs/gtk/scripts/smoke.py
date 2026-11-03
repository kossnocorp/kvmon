"""D-Bus integration test; all DDC commands go to a temporary fake executable."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
NAME = "io.github.kvmon.Monitor"


def call(method, *args, check=True):
    return subprocess.run(
        ["gdbus", "call", "--session", "--dest", NAME,
         "--object-path", "/io/github/kvmon/Monitor",
         "--method", f"{NAME}.{method}", *args],
        capture_output=True, text=True, check=check, timeout=5,
    )


def wait_for(text):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        result = call("GetState", check=False)
        if result.returncode == 0 and text in result.stdout:
            return result.stdout
        time.sleep(0.05)
    raise AssertionError(f"Did not observe {text}: {result}")


def test():
    with tempfile.TemporaryDirectory(prefix="kvmon-") as directory:
        tmp = Path(directory)
        fake = tmp / "ddcutil"
        fake.write_text(f"#!{sys.executable}\n" + '''
import os
from pathlib import Path
import sys
import time
p = Path(os.environ["FAKE_MONITOR"])
args = sys.argv[1:]
if (p / "disconnected").exists():
    sys.exit("Monitor disconnected")
if args == ["detect"]:
    print("Display 1\\n I2C bus: /dev/i2c-42\\n Mfg id: DEL\\n Model: DELL U4025QW\\n Serial number: FAKE")
elif "getvcp" in args:
    if (p / "slow-read").exists():
        time.sleep(0.3)
    print("VCP 60 SNC x" + (p / "input").read_text())
elif "setvcp" in args:
    with (p / "writes").open("a") as log:
        log.write(args[-1] + "\\n")
    time.sleep(0.2)
    if (p / "lose-on-write").exists():
        (p / "disconnected").touch()
    else:
        (p / "input").write_text(args[-1][2:])
else:
    sys.exit("Unexpected command")
''')
        fake.chmod(0o755)
        (tmp / "input").write_text("0f")
        env = dict(os.environ, KVMON_DDCUTIL=str(fake), FAKE_MONITOR=str(tmp), XDG_CONFIG_HOME=str(tmp / "config"))
        env.pop("KVMON_DISPLAY_SERIAL", None)
        process = subprocess.Popen([str(ROOT / "target/debug/kvmon-gtk")], env=env)
        try:
            state = wait_for("'ready'")
            assert "uint32 15" in state and "Thunderbolt" in state and "DisplayPort-1" in state
            assert "DELL U4025QW" in call("GetMonitor").stdout
            assert "42" in call("GetMonitor").stdout
            call("SetAlias", "25", "Work laptop")
            assert "Work laptop" in call("GetState").stdout
            assert call("SetAlias", "17", "Unknown", check=False).returncode != 0
            call("SetAlias", "25", "")
            assert "Thunderbolt" in call("GetState").stdout
            call("SetAlias", "15", "Desktop")
            assert "Desktop" in call("GetState").stdout
            (tmp / "slow-read").touch()
            call("Refresh")
            assert "'ready'" in call("GetState").stdout, "Refresh must retain observed state"
            assert "Desktop" in call("GetState").stdout
            time.sleep(0.5)
            (tmp / "slow-read").unlink()
            call("Select", "15")
            assert not (tmp / "writes").exists(), "Active selection must not write"
            assert call("Select", "17", check=False).returncode != 0
            (tmp / "slow-read").touch()
            call("Refresh")
            call("Select", "25")
            assert "'switching'" in call("GetState").stdout
            assert call("Select", "15", check=False).returncode != 0
            assert "uint32 25" in wait_for("'ready'")
            (tmp / "slow-read").unlink()
            (tmp / "lose-on-write").touch()
            call("Select", "15")
            state = wait_for("'unavailable'")
            assert "uint32 0" in state, "Failed readback must clear the checkmark"
            (tmp / "disconnected").unlink()
            call("Refresh")
            assert "uint32 25" in wait_for("'ready'")
            assert (tmp / "writes").read_text().splitlines() == ["0x19", "0x0f"]
            call("Quit")
            assert process.wait(timeout=5) == 0
            process = subprocess.Popen([str(ROOT / "target/debug/kvmon-gtk")], env=env)
            assert "Desktop" in wait_for("'ready'")
            call("Quit")
            assert process.wait(timeout=5) == 0
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
    print("D-Bus smoke passed: discovery, no-op, validation, switching, failed readback, recovery, Quit")


if __name__ == "__main__":
    if "--isolated" in sys.argv:
        test()
    else:
        subprocess.run(["cargo", "build", "-p", "kvmon-gtk"], cwd=ROOT, check=True)
        # Hide any installed activation file before starting the isolated bus.
        with tempfile.TemporaryDirectory(prefix="kvmon-bus-") as data:
            subprocess.run(["dbus-run-session", "--", sys.executable, __file__, "--isolated"],
                           env=dict(os.environ, XDG_DATA_HOME=data), check=True)
