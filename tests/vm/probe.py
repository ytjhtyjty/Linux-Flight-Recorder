
import subprocess

def ssh_ready(argv, *, run=subprocess.run, timeout=8):
    try:
        return run(argv, capture_output=True, timeout=timeout).returncode == 0
    except subprocess.TimeoutExpired:
        return False

def retryable_ssh_exit(code):

    return code == 255
