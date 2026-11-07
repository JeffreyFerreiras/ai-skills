"""Inspect only the approved System32 CRT error-conversion exports."""

import argparse
import ctypes
import hashlib
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    kernel = ctypes.WinDLL("kernel32.dll", winmode=0x800)
    kernel.GetSystemDirectoryW.argtypes = (ctypes.c_wchar_p, ctypes.c_uint)
    kernel.GetSystemDirectoryW.restype = ctypes.c_uint
    buffer = ctypes.create_unicode_buffer(32768)
    length = kernel.GetSystemDirectoryW(buffer, len(buffer))
    if not length or length >= len(buffer):
        raise OSError("SYSTEM32_RESOLUTION_FAILED")
    system = Path(buffer.value).resolve(strict=True)
    records = []
    for name in ("ucrtbase.dll", "msvcrt.dll"):
        path = (system / name).resolve(strict=True)
        if path.parent != system:
            raise ValueError("CRT_OUTSIDE_SYSTEM32")
        library = ctypes.CDLL(str(path), winmode=0x800)
        exports = {}
        for symbol in ("_dosmaperr", "__acrt_errno_map_os_error", "_errno", "strerror"):
            try:
                exports[symbol] = getattr(library, symbol)
            except AttributeError:
                exports[symbol] = None
        record = dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest(), exports={key: value is not None for key, value in exports.items()}, mapped=[])
        mapper = exports["_dosmaperr"] or exports["__acrt_errno_map_os_error"]
        if mapper is not None and exports["_errno"] is not None and exports["strerror"] is not None:
            mapper.argtypes = (ctypes.c_ulong,)
            mapper.restype = None
            exports["_errno"].argtypes = ()
            exports["_errno"].restype = ctypes.POINTER(ctypes.c_int)
            exports["strerror"].argtypes = (ctypes.c_int,)
            exports["strerror"].restype = ctypes.c_char_p
            pointer = exports["_errno"]()
            before = pointer[0]
            try:
                for code in (2, 3, 5, 6, 123):
                    mapper(code)
                    record["mapped"].append(dict(winerror=code, errno=pointer[0], description=exports["strerror"](pointer[0]).decode("ascii")))
            finally:
                pointer[0] = before
        records.append(record)
    report = dict(scope="approved actual System32 CRT ABI probe; no installation or persistent change", records=records)
    with args.output.open("x", encoding="ascii") as stream:
        stream.write(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
