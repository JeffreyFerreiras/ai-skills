"""Read the alternate data streams that Windows stores alongside a file."""

import ctypes
import hashlib
import os
import shutil
import stat
from ctypes import wintypes
from pathlib import Path


class StreamData(ctypes.Structure):
    _fields_ = [("size", ctypes.c_longlong), ("name", wintypes.WCHAR * 296)]


kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
find_first = kernel32.FindFirstStreamW
find_first.argtypes = (wintypes.LPCWSTR, wintypes.DWORD, ctypes.POINTER(StreamData), wintypes.DWORD)
find_first.restype = wintypes.HANDLE
find_next = kernel32.FindNextStreamW
find_next.argtypes = (wintypes.HANDLE, ctypes.POINTER(StreamData))
find_next.restype = wintypes.BOOL
find_close = kernel32.FindClose
find_close.argtypes = (wintypes.HANDLE,)
find_close.restype = wintypes.BOOL

INVALID_HANDLE = ctypes.c_void_p(-1).value
ERROR_HANDLE_EOF = 38
ERROR_INVALID_PARAMETER = 87


def stream_names(path: Path) -> tuple[str, ...]:
    streams = StreamData()
    handle = find_first(str(path), 0, ctypes.byref(streams), 0)
    if handle == INVALID_HANDLE:
        error = ctypes.get_last_error()
        if error in (ERROR_HANDLE_EOF, ERROR_INVALID_PARAMETER):
            return ()
        raise ctypes.WinError(error)

    names = []
    try:
        while True:
            if streams.name != "::$DATA":
                names.append(streams.name)
            if not find_next(handle, ctypes.byref(streams)):
                error = ctypes.get_last_error()
                if error != ERROR_HANDLE_EOF:
                    raise ctypes.WinError(error)
                break
    finally:
        find_close(handle)
    return tuple(sorted(names))


def stream_digests(path: Path) -> tuple[tuple[str, str], ...]:
    digests = []
    for name in stream_names(path):
        with open(f"{path}{name}", "rb") as stream:
            digests.append((name, hashlib.file_digest(stream, "sha256").hexdigest()))
    return tuple(digests)


def copy_streams(source: Path, target: Path) -> None:
    existing = stream_names(target)
    for name in stream_names(source):
        if name not in existing:
            with open(f"{source}{name}", "rb") as incoming, open(f"{target}{name}", "wb") as outgoing:
                shutil.copyfileobj(incoming, outgoing)


def copy_directory_streams(source: Path, target: Path) -> None:
    for directory, _, _ in os.walk(source):
        original = Path(directory)
        copied = target / original.relative_to(source)
        mode = stat.S_IMODE(copied.stat().st_mode)
        if not mode & stat.S_IWRITE:
            copied.chmod(mode | stat.S_IWRITE)
        try:
            copy_streams(original, copied)
        finally:
            shutil.copystat(original, copied)
