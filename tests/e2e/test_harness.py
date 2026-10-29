#!/usr/bin/env python3

import pathlib
import subprocess
import sys
import unittest
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'vm'))
from probe import ssh_ready, retryable_ssh_exit

class ReadinessTests(unittest.TestCase):
    def test_success(self):
        self.assertTrue(ssh_ready(['ssh'],run=lambda *a,**k:subprocess.CompletedProcess(a,0)))
    def test_not_ready(self):
        self.assertFalse(ssh_ready(['ssh'],run=lambda *a,**k:subprocess.CompletedProcess(a,255)))
    def test_timeout_is_retryable_not_overall_failure(self):
        def slow(*a,**k):raise subprocess.TimeoutExpired(a[0],k['timeout'])
        self.assertFalse(ssh_ready(['ssh'],run=slow))
    def test_only_transport_exit_is_retryable(self):
        self.assertTrue(retryable_ssh_exit(255))
        for code in [0,1,2,3,130]:self.assertFalse(retryable_ssh_exit(code))
    def test_termination_is_not_swallowed(self):
        def interrupted(*a,**k):raise RuntimeError('termination requested')
        with self.assertRaisesRegex(RuntimeError,'termination'):ssh_ready(['ssh'],run=interrupted)
if __name__=='__main__':unittest.main()
