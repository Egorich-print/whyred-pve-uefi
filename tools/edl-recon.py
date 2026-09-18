#!/usr/bin/env python3
"""Sahara v2 + Firehose client for reading devinfo from EDL 9008 (SDM660).
Uses pyusb directly — rusb has macOS compatibility issues with QUSB__BULK."""

import usb.core, struct, time, sys, os

QCOM_VID, QCOM_PID = 0x05C6, 0x9008
LOADER = os.path.expanduser("~/ai-workstation/Tools/edl/loaders-local/jasmine_prog_emmc_firehose_Sdm660_ddr.elf")

def le32(b, o): return struct.unpack_from('<I', b, o)[0]
def put32(b, o, v): struct.pack_into('<I', b, o, v)

# Sahara commands
HELLO=0x01; HELLO_RESP=0x02; CMD_READ=0x03; END=0x04; DONE=0x05; DONE_RESP=0x06; RESET=0x07; EXEC=0x0B; EXEC_RESP=0x0C

def find_device():
    dev = usb.core.find(idVendor=QCOM_VID, idProduct=QCOM_PID)
    if not dev:
        print("no EDL device (05c6:9008)"); sys.exit(1)
    try: dev.set_configuration()
    except: pass
    return dev

def sahara_handshake(dev):
    """Receive HELLO, send HELLO_RESP, return device mode."""
    hello = bytes(dev.read(0x81, 48, timeout=5000))
    cmd, version, mode = le32(hello,0), le32(hello,8), le32(hello,20)
    assert cmd == HELLO, f"expected HELLO, got {cmd:#x}"
    print(f"  HELLO: v{version} mode={mode}")

    resp = bytearray(48)
    put32(resp, 0, HELLO_RESP)
    put32(resp, 4, 48)
    put32(resp, 8, version)
    put32(resp, 12, 1)  # version_min
    put32(resp, 16, 4096)  # max_cmd
    put32(resp, 20, mode)  # match device mode
    put32(resp, 24, 0)  # supported mode 0
    put32(resp, 28, 1)  # supported mode 1
    dev.write(0x01, bytes(resp))
    return mode

def sahara_upload_loader(dev, loader_path):
    """Upload firehose loader via Sahara CMD_READ protocol."""
    loader = open(loader_path, 'rb').read()
    print(f"  loader: {len(loader)} bytes")

    while True:
        pkt = bytes(dev.read(0x81, 4096, timeout=10000))
        cmd = le32(pkt, 0)
        if cmd in (CMD_READ, 0x0E):  # CMD_READ or CMD_READ_DATA
            offset, length = le32(pkt, 8), le32(pkt, 12)
            print(f"  CMD_READ: offset={offset:#x} len={length}")
            dev.write(0x01, loader[offset:offset+length])
        elif cmd == END:
            print("  END — sending DONE")
            done = bytearray(8)
            put32(done, 0, DONE)
            put32(done, 4, 8)
            dev.write(0x01, bytes(done))
            try:
                r = bytes(dev.read(0x81, 64, timeout=3000))
                print(f"  DONE resp: cmd={le32(r,0):#x}")
            except: pass
            return True
        elif cmd == RESET:
            print("  RESET — loader accepted")
            return True
        else:
            print(f"  unexpected cmd={cmd:#x}")

def firehose_cmd(dev, cmd_xml, timeout=5000):
    """Send firehose XML command, read response (XML or raw binary)."""
    payload = cmd_xml.encode() + b"\n"
    dev.write(0x01, payload, timeout)
    resp = b""
    for _ in range(30):
        try:
            chunk = bytes(dev.read(0x81, 4096, timeout=timeout))
            resp += chunk
            # check for XML response
            text = resp.decode(errors='replace')
            if "<data>" in text or "ACK" in text or "NAK" in text:
                return text
            # check for raw firehose ACK (0x00 repeated or specific pattern)
            if len(resp) >= 4:
                # raw firehose: first byte might be command ID
                return f"raw:{resp[:64].hex()}"
        except usb.core.USBTimeoutError:
            if resp:
                return f"partial:{resp.hex()}"
            break
        except Exception as e:
            return f"error:{e}"
    return f"no_response:{resp.hex() if resp else 'empty'}"

def firehose_read_raw(dev, phys_part, start_sector, num_sectors, sector_size=512, timeout=10000):
    """Read sectors via firehose raw read command."""
    cmd = (f'<?xml version="1.0" ?><data>'
           f'<read SECTOR_SIZE_IN_BYTES="{sector_size}" '
           f'num_partition_sectors="{num_sectors}" '
           f'physical_partition_number="{phys_part}" '
           f'start_sector="{start_sector}" />'
           f'</data>')
    dev.write(0x01, cmd.encode(), timeout)
    resp = b""
    for _ in range(60):
        try:
            chunk = bytes(dev.read(0x81, 65536, timeout=timeout))
            resp += chunk
            text = resp.decode(errors='replace')
            if "<data>" in text and ("ACK" in text or "NAK" in text):
                return resp
        except usb.core.USBTimeoutError:
            break
        except Exception as e:
            break
    return resp

def main():
    print("[1] Finding EDL device...")
    dev = find_device()
    print("  found!")

    print("[2] Sahara handshake...")
    mode = sahara_handshake(dev)

    print("[3] Uploading loader...")
    sahara_upload_loader(dev, LOADER)
    print("  loader uploaded! Device should re-enumerate...")
    time.sleep(2)

    # device may have new endpoints after loader execution
    # try firehose on same endpoints first
    print("[4] Firehose: send sync...")
    r = firehose_cmd(dev, '<?xml version="1.0" ?><data><nop /></data>')
    print(f"  sync response: {r[:200]}")

    print("[5] Firehose: print GPT...")
    r = firehose_cmd(dev, '<?xml version="1.0" ?><data><configure MemoryName="emmc" verbose="0" MaxPayloadSizeToTargetInBytes="1048576" /></data>')
    print(f"  configure: {r[:200]}")

    r = firehose_cmd(dev, '<?xml version="1.0" ?><data><command>DumpGPT</command></data>')
    print(f"  GPT: {r[:500]}")

    print("[6] Firehose: read devinfo (sector 0, 64 sectors)...")
    r = firehose_cmd(dev, '<?xml version="1.0" /><data><read SECTOR_SIZE_IN_BYTES="512" num_partition_sectors="64" physical_partition_number="0" start_sector="0" /></data>')
    print(f"  devinfo: {r[:500]}")

if __name__ == "__main__":
    main()
