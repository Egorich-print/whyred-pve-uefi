#!/usr/bin/env python3
"""Sahara v2 loader upload for EDL 9008 via pyusb.

Scope: upload the Firehose loader and verify the device accepted it.
It never issues program/erase commands and never writes to storage.

After a successful upload the device re-enumerates; use the maintained
bkerler/edl client for the Firehose reads (GPT, devinfo):

    cd ~/ai-workstation/Tools/edl
    ./venv-edl/bin/edl r gpt
    ./venv-edl/bin/edl r devinfo devinfo.bin

Why pyusb and not tools/sahara-rs (rusb): on this macOS host the QUSB__BULK
interface does not enumerate through rusb, while pyusb transfers work.
Both speak the same protocol; this file is the pyusb path.
"""

import argparse
import datetime
import os
import struct
import sys

try:
    import usb.core
except ImportError:
    sys.exit("pyusb is required: pip install pyusb")

QCOM_VID, QCOM_PID = 0x05C6, 0x9008
EP_IN, EP_OUT = 0x81, 0x01

HELLO_REQ, HELLO_RSP = 0x01, 0x02
READ_DATA, END_TRANSFER = 0x03, 0x04
DONE_REQ, DONE_RSP = 0x05, 0x06
RESET_RSP, CMD_READY = 0x08, 0x0B
STATUS_SUCCESS = 0x00


def le32(b, o):
    return struct.unpack_from("<I", b, o)[0]


def put32(b, o, v):
    struct.pack_into("<I", b, o, v)


def write_exact(dev, data):
    """pyusb may accept fewer bytes than offered; a short write is a protocol
    error, not something to discover three packets later."""
    written = dev.write(EP_OUT, data)
    if written != len(data):
        raise ProtocolError(f"short USB write: {written} of {len(data)} bytes")


def read_exact(dev, size, timeout):
    data = b""
    while len(data) < size:
        chunk = bytes(dev.read(EP_IN, size - len(data), timeout=timeout))
        if not chunk:
            break
        data += chunk
    return data


def find_device():
    devices = list(usb.core.find(idVendor=QCOM_VID, idProduct=QCOM_PID, find_all=True))
    if not devices:
        sys.exit(f"no EDL device ({QCOM_VID:#06x}:{QCOM_PID:#04x}) — power-cycle into 9008 first")
    if len(devices) > 1:
        sys.exit(f"{len(devices)} EDL devices attached — disconnect all but one")
    dev = devices[0]
    try:
        dev.set_configuration()
    except usb.core.USBError:
        pass
    return dev


def handshake(dev):
    hello = read_exact(dev, 48, 5000)
    if len(hello) < 24:
        sys.exit(f"short HELLO ({len(hello)} bytes)")
    cmd, version, max_cmd_len, mode = le32(hello, 0), le32(hello, 8), le32(hello, 16), le32(hello, 20)
    if cmd != HELLO_REQ:
        sys.exit(f"expected HELLO, got {cmd:#x}")
    if not 1 <= version <= 3:
        sys.exit(f"unexpected Sahara version {version}")
    max_cmd_len = min(max(max_cmd_len, 256), 1 << 20)
    print(f"  HELLO: version={version} max_cmd_len={max_cmd_len} mode={mode}")

    resp = bytearray(48)
    put32(resp, 0, HELLO_RSP)
    put32(resp, 4, 48)
    put32(resp, 8, version)
    put32(resp, 12, 1)
    put32(resp, 16, max_cmd_len)
    put32(resp, 20, mode)
    for i, v in enumerate((1, 2, 3, 4, 5, 6)):
        put32(resp, 24 + i * 4, v)
    write_exact(dev, bytes(resp))
    return max_cmd_len


class ProtocolError(Exception):
    """Device violated the Sahara protocol, or refused the loader."""


SEND, WAIT, FINISH = "send", "wait", "finish"


def step(pkt, loader):
    """Pure state machine: one device packet in, (action, payload) out.

    Mirrors tools/sahara-rs so both uploaders accept the same transcripts.
    Raises ProtocolError instead of guessing — a wrong loader must fail loudly.
    """
    if len(pkt) < 8:
        raise ProtocolError(f"short packet: {len(pkt)} bytes")
    cmd = le32(pkt, 0)
    if cmd == READ_DATA:
        if len(pkt) < 20:
            raise ProtocolError(f"short READ_DATA: {len(pkt)} bytes")
        image, offset, length = le32(pkt, 8), le32(pkt, 12), le32(pkt, 16)
        if image != 0:
            raise ProtocolError(f"device requested image id {image}, only 0 supported")
        if offset + length > len(loader):
            raise ProtocolError(f"loader too short: need {offset + length}, have {len(loader)}")
        return SEND, loader[offset:offset + length], (offset, length)
    if cmd == END_TRANSFER:
        if len(pkt) < 16:
            raise ProtocolError(f"short END_TRANSFER: {len(pkt)} bytes")
        status = le32(pkt, 12)
        if status != STATUS_SUCCESS:
            raise ProtocolError(f"device reported transfer failure (status {status:#x})")
        done = bytearray(12)
        put32(done, 0, DONE_REQ)
        put32(done, 4, 12)
        put32(done, 8, STATUS_SUCCESS)
        return SEND, bytes(done), None
    if cmd == DONE_RSP:
        if len(pkt) < 12:
            raise ProtocolError(f"short DONE_RSP: {len(pkt)} bytes")
        status = le32(pkt, 8)
        if status != STATUS_SUCCESS:
            raise ProtocolError(f"loader rejected (DONE_RSP status {status:#x})")
        return FINISH, None, None
    if cmd == RESET_RSP:
        return FINISH, None, None
    if cmd in (CMD_READY, HELLO_REQ):
        return WAIT, None, None
    raise ProtocolError(f"unexpected Sahara command {cmd:#x}")


def upload(dev, loader, max_cmd_len):
    """Drive step() over real USB. Returns when the loader is accepted."""
    pkt = read_exact(dev, max_cmd_len, 15000)
    while True:
        try:
            action, payload, info = step(pkt, loader)
        except ProtocolError as e:
            sys.exit(f"protocol error: {e}")
        if action == SEND:
            if info:
                print(f"  READ_DATA offset={info[0]:#x} len={info[1]}")
            else:
                print("  END — sending DONE")
            try:
                write_exact(dev, payload)
            except ProtocolError as e:
                sys.exit(f"protocol error: {e}")
            if info is None:
                # DONE was sent: the acceptance verdict is the next packet
                pkt = read_exact(dev, 64, 5000)
                if len(pkt) < 12:
                    sys.exit("no DONE_RSP — loader acceptance unconfirmed")
                action, _, _ = _safe_step(pkt, loader)
                if action == FINISH:
                    return
                pkt = read_exact(dev, max_cmd_len, 15000)
                continue
        elif action == FINISH:
            return
        pkt = read_exact(dev, max_cmd_len, 15000)
        if len(pkt) < 8:
            sys.exit("device stopped sending commands mid-upload")


def _safe_step(pkt, loader):
    try:
        return step(pkt, loader)
    except ProtocolError as e:
        sys.exit(f"protocol error: {e}")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--loader", required=True, help="Firehose loader for THIS device's SoC")
    ap.add_argument("--log-dir", default=None, help="where to write the transcript (default: stdout only)")
    args = ap.parse_args()

    path = os.path.expanduser(args.loader)
    if not os.path.isfile(path):
        sys.exit(f"loader not found: {path}")
    loader = open(path, "rb").read()
    if not loader:
        sys.exit(f"loader is empty: {path}")
    print(f"loader: {path} ({len(loader)} bytes)")
    print("NOTE: a loader built for another SoC can wedge EDL until the next power cycle")

    dev = find_device()
    max_cmd_len = handshake(dev)
    upload(dev, loader, max_cmd_len)

    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    print("loader accepted")
    if args.log_dir:
        os.makedirs(args.log_dir, exist_ok=True)
        log = os.path.join(args.log_dir, f"edl-upload-{stamp}.log")
        with open(log, "w") as f:
            f.write(f"loader={path} size={len(loader)} accepted\n")
        print(f"transcript: {log}")

    print("the device should re-enumerate; give it a few seconds, then:")
    print("  cd ~/ai-workstation/Tools/edl && ./venv-edl/bin/edl r gpt")


if __name__ == "__main__":
    main()
