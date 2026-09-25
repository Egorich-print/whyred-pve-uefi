#!/usr/bin/env python3
"""Unit tests for the offline-testable parts of the EDL/devinfo tooling.

Run: python3 tools/test_tools.py
Covers tools/edl-recon.py (Sahara state machine) and tools/analyze-devinfo.py.
These are pure functions: no USB, no device, no root.
"""
import importlib.util
import os
import struct
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, os.path.join(HERE, path))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


edl = load("edl_recon", "edl-recon.py")
adv = load("analyze_devinfo", "analyze-devinfo.py")

put32 = edl.put32
le32 = edl.le32


def pkt(cmd, length, fields):
    buf = bytearray(length)
    put32(buf, 0, cmd)
    put32(buf, 4, length)
    for off, val in fields:
        put32(buf, off, val)
    return bytes(buf)


class SaharaStateMachine(unittest.TestCase):
    LOADER = bytes(range(256)) * 16  # 4096 bytes

    def test_full_upload_transcript(self):
        p = pkt(edl.READ_DATA, 20, [(8, 0), (12, 0), (16, 80)])
        action, payload, info = edl.step(p, self.LOADER)
        self.assertEqual(action, edl.SEND)
        self.assertEqual(payload, self.LOADER[:80])
        self.assertEqual(info, (0, 80))

        p = pkt(edl.READ_DATA, 20, [(8, 0), (12, 4088), (16, 8)])
        action, payload, _ = edl.step(p, self.LOADER)
        self.assertEqual(action, edl.SEND)
        self.assertEqual(payload, self.LOADER[4088:])

        p = pkt(edl.END_TRANSFER, 16, [(8, 0), (12, 0)])
        action, payload, _ = edl.step(p, self.LOADER)
        self.assertEqual(action, edl.SEND)
        self.assertEqual(le32(payload, 0), edl.DONE_REQ)
        self.assertEqual(le32(payload, 4), 12)
        self.assertEqual(le32(payload, 8), 0)

        p = pkt(edl.DONE_RSP, 12, [(8, 0)])
        self.assertEqual(edl.step(p, self.LOADER)[0], edl.FINISH)

    def test_failure_statuses_never_report_success(self):
        end_fail = pkt(edl.END_TRANSFER, 16, [(8, 0), (12, 1)])
        with self.assertRaises(edl.ProtocolError):
            edl.step(end_fail, self.LOADER)
        done_fail = pkt(edl.DONE_RSP, 12, [(8, 2)])
        with self.assertRaises(edl.ProtocolError):
            edl.step(done_fail, self.LOADER)
        done_fail_nonzero_cmd = pkt(edl.CMD_READY, 12, [(8, 0)])
        self.assertEqual(edl.step(done_fail_nonzero_cmd, self.LOADER)[0], edl.WAIT)

    def test_malformed_requests(self):
        with self.assertRaises(edl.ProtocolError):
            edl.step(b"\x01\x02\x03\x04", self.LOADER)  # too short
        with self.assertRaises(edl.ProtocolError):
            edl.step(pkt(edl.READ_DATA, 20, [(8, 0), (12, 4090), (16, 80)]), self.LOADER)
        with self.assertRaises(edl.ProtocolError):
            edl.step(pkt(edl.READ_DATA, 20, [(8, 1), (12, 0), (16, 80)]), self.LOADER)
        with self.assertRaises(edl.ProtocolError):
            edl.step(pkt(edl.READ_DATA, 12, [(8, 0)]), self.LOADER)
        with self.assertRaises(edl.ProtocolError):
            edl.step(pkt(0xBEEF, 8, []), self.LOADER)

    def test_reset_response_is_accepted(self):
        self.assertEqual(edl.step(pkt(edl.RESET_RSP, 8, []), self.LOADER)[0], edl.FINISH)

    def test_matches_rust_implementation_constants(self):
        # both uploaders must agree on the wire protocol
        self.assertEqual(
            (edl.HELLO_REQ, edl.HELLO_RSP, edl.READ_DATA, edl.END_TRANSFER,
             edl.DONE_REQ, edl.DONE_RSP, edl.RESET_RSP, edl.CMD_READY),
            (0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x8, 0xB),
        )


class AnalyzeDevinfo(unittest.TestCase):
    def test_nonzero_regions(self):
        data = bytes(16) + b"\x01" * 32 + bytes(8) + b"\x02" * 4 + bytes(4) + b"\x03" * 20
        regions = adv.nonzero_regions(data, min_run=16)
        self.assertEqual(regions, [(16, 48), (64, 84)])  # the 4-byte run is below min_run

    def test_keyword_hits_are_case_insensitive_and_longest_match_wins(self):
        data = b"\x00" * 8 + b"is_unlocked" + b"UNLOCKED" + b"\xff" * 4
        hits = adv.keyword_hits(data)
        self.assertEqual(hits, [(8, b"is_unlocked"), (19, b"unlocked")])

    def test_generic_substrings_are_not_reported(self):
        data = b"redmi" + b"antivirus" + b"flashable"
        self.assertEqual(adv.keyword_hits(data), [])


if __name__ == "__main__":
    sys.exit(0 if unittest.main(exit=False, verbosity=2).result.wasSuccessful() else 1)
