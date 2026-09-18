#!/usr/bin/env python3
"""EDL devinfo dumper: Sahara v2 + firehose via pyusb.
Dumps GPT, finds devinfo partition, reads it."""

import usb.core, struct, time, sys, os, uuid

QCOM_VID, QCOM_PID = 0x05C6, 0x9008
LOADER = os.path.expanduser("~/ai-workstation/Tools/edl/loaders-local/jasmine_prog_emmc_firehose_Sdm660_ddr.elf")

def le32(b, o): return struct.unpack_from('<I', b, o)[0]
def le64(b, o): return struct.unpack_from('<Q', b, o)[0]
def put32(b, o, v): struct.pack_into('<I', b, o, v)

HELLO=0x01; HELLO_RESP=0x02; CMD_READ=0x03; END=0x04; DONE=0x05; RESET=0x07

def find_device():
    dev = usb.core.find(idVendor=QCOM_VID, idProduct=QCOM_PID)
    if not dev:
        print("no EDL device (05c6:9008)"); sys.exit(1)
    try: dev.set_configuration()
    except: pass
    return dev

def sahara_handshake(dev):
    hello = bytes(dev.read(0x81, 48, timeout=5000))
    cmd, version, mode = le32(hello,0), le32(hello,8), le32(hello,20)
    if cmd != HELLO:
        raise ValueError(f"bad HELLO cmd={cmd:#x}")
    print(f"  HELLO: v{version} mode={mode}")
    resp = bytearray(48)
    put32(resp, 0, HELLO_RESP)
    put32(resp, 4, 48)
    put32(resp, 8, version)
    put32(resp, 12, 1)
    put32(resp, 16, 4096)
    put32(resp, 20, mode)
    put32(resp, 24, 0)
    put32(resp, 28, 1)
    dev.write(0x01, bytes(resp))
    return mode

def sahara_upload_loader(dev, loader_path):
    loader = open(loader_path, 'rb').read()
    print(f"  loader: {len(loader)} bytes")
    responses = []
    while True:
        pkt = bytes(dev.read(0x81, 4096, timeout=15000))
        cmd = le32(pkt, 0)
        if cmd in (CMD_READ, 0x0E):
            offset, length = le32(pkt, 8), le32(pkt, 12)
            print(f"  CMD_READ: offset={offset:#x} len={length}")
            dev.write(0x01, loader[offset:offset+length])
        elif cmd == END:
            print("  END — sending DONE")
            done = bytearray(8); put32(done,0,DONE); put32(done,4,8)
            dev.write(0x01, bytes(done))
            try:
                r = bytes(dev.read(0x81, 64, timeout=3000))
                print(f"  DONE resp: cmd={le32(r,0):#x}")
                responses.append(r)
            except: pass
            # After DONE, device should switch to firehose mode (re-enumerate or stay same)
            return True
        elif cmd == RESET:
            print("  RESET — loader accepted")
            return True

def fh_send(dev, xml_cmd, timeout=5000):
    """Send firehose XML command, return raw response bytes."""
    payload = (xml_cmd + "\n").encode()
    dev.write(0x01, payload, timeout)
    resp = b""
    for _ in range(100):
        try:
            chunk = bytes(dev.read(0x81, 65536, timeout=timeout))
            resp += chunk
            text = resp.decode(errors='replace')
            if "ACK" in text or "NAK" in text or "</data>" in text or "<data>" in text:
                if len(chunk) < 100:  # small response = status
                    return text, resp
                break  # might be data response, keep reading
            if len(chunk) < 100:
                return f"short_raw:{resp.hex()}", resp
        except usb.core.USBTimeoutError:
            if resp:
                return f"timeout:{resp.hex()}", resp
            return "no_response", b""
        except Exception as e:
            return f"error:{e}", resp
    return f"partial:{resp.hex()}", resp

def fh_read_sectors(dev, start_sector, num_sectors, sector_size=512, timeout=10000):
    """Read eMMC sectors via firehose."""
    cmd = (f'<?xml version="1.0" ?><data>'
           f'<read SECTOR_SIZE_IN_BYTES="{sector_size}" '
           f'num_partition_sectors="{num_sectors}" '
           f'physical_partition_number="0" '
           f'start_sector="{start_sector}" />'
           f'</data>')
    print(f"  READ: start={start_sector} count={num_sectors}")
    dev.write(0x01, cmd.encode(), timeout)
    resp = b""
    for _ in range(200):
        try:
            chunk = bytes(dev.read(0x81, 65536, timeout=timeout))
            resp += chunk
        except usb.core.USBTimeoutError:
            break
        except Exception as e:
            break
    return resp

def parse_gpt(data, page_size=512):
    """Parse GPT from first 2 pages (header + entries)."""
    # GPT header at sector 1 (LBA 1), offset 0 in the page
    hdr = data[page_size:]  # skip protective MBR
    if hdr[0:8] != b'EFI PART':
        print("  GPT header not found at LBA 1")
        return []
    # parse GPT header
    hlen = le32(hdr, 0)  # header size
    part_entry_lba = le64(hdr, 72)  # partition entry array LBA
    part_entry_count = le32(hdr, 80)
    part_entry_size = le32(hdr, 84)
    print(f"  GPT: header_size={hlen} entries@{part_entry_lba} count={part_entry_count} entry_size={part_entry_size}")

    # read partition entries
    # GPT entries start at LBA = part_entry_lba
    # But if we only read 64 pages (sector 0-63), they're within first 32KB
    # entries start at byte offset 512*part_entry_lba within our data
    entries_start = page_size * part_entry_lba
    if entries_start >= len(data):
        print(f"  partition entries at LBA {part_entry_lba} not in dump range")
        return []

    entries = []
    for i in range(min(part_entry_count, 128)):
        off = entries_start + i * part_entry_size
        if off + part_entry_size > len(data):
            break
        entry = data[off:off+part_entry_size]
        type_guid = entry[0:16]
        if all(b == 0 for b in type_guid):
            continue
        first_lba = le64(entry, 32)
        last_lba = le64(entry, 40)
        name = entry[56:128].decode('utf-16-le', errors='replace').rstrip('\x00')
        entries.append((name, first_lba, last_lba))
    return entries

def main():
    print("[1] Finding EDL device...")
    dev = find_device()
    print("  found!")

    print("[2] Sahara handshake...")
    sahara_handshake(dev)

    print("[3] Uploading loader...")
    sahara_upload_loader(dev, LOADER)
    print("  loader uploaded!")
    time.sleep(2)

    print("[4] Firehose: get GPT...")
    # send firehose commands to initialize
    resp_text, resp_raw = fh_send(dev, '<?xml version="1.0"?><data><configure MemoryName="emmc" verbose="0" MaxPayloadSizeToTargetInBytes="1048576" /></data>')
    print(f"  configure resp: {resp_text[:200]}")

    resp_text, _ = fh_send(dev, '<?xml version="1.0"?><data><nop/></data>')
    print(f"  nop resp: {resp_text[:200]}")

    print("[5] Reading GPT (sectors 0-63, 32KB)...")
    gpt_data = fh_read_sectors(dev, 0, 64, 512, 10000)
    print(f"  GPT data: {len(gpt_data)} bytes")

    if len(gpt_data) >= 32768:
        print("[6] Parsing GPT...")
        partitions = parse_gpt(gpt_data)
        for name, start_lba, end_lba in partitions:
            print(f"  {name}: LBA {start_lba} ({start_lba*512:#x}) — {end_lba} ({end_lba*512:#x}) size={((end_lba-start_lba+1)*512)//1024}KB")
            if "devinfo" in name.lower():
                print(f"  *** FOUND devinfo at LBA {start_lba} ***")
                # read devinfo partition (usually small, 4KB)
                print("[7] Reading devinfo...")
                devinfo_raw = fh_read_sectors(dev, start_lba, 8, 512, 10000)
                print(f"\n=== DEVINFO ({len(devinfo_raw)} bytes) ===")
                for i in range(0, len(devinfo_raw), 16):
                    hex_line = ' '.join(f"{b:02x}" for b in devinfo_raw[i:i+16])
                    ascii_line = ''.join(chr(b) if 32 <= b < 127 else '.' for b in devinfo_raw[i:i+16])
                    print(f"{i:08x}: {hex_line:<48} {ascii_line}")
                # check magic
                if len(devinfo_raw) >= 13:
                    magic = devinfo_raw[0:13]
                    print(f"\nMagic: {magic}")

    else:
        print("  GPT data too short!")

    # save raw data
    with open("/tmp/lavender-devinfo-raw.bin", "wb") as f:
        f.write(gpt_data)
    print(f"\nGPT saved → /tmp/lavender-devinfo-raw.bin")
    with open("/tmp/lavender-devinfo-full.bin", "wb") as f:
        f.write(resp_raw)
    print(f"Full firehose response saved → /tmp/lavender-devinfo-full.bin")

if __name__ == "__main__":
    main()
