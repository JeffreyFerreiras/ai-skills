"""Remove an owned fixture, including readonly Git object files."""

import os
from pathlib import Path
import shutil
import stat


def remove_owned_fixture(root, fixtures):
    root = Path(root).resolve(strict=True)
    fixtures = Path(fixtures).resolve(strict=True)
    if root.parent != fixtures:
        raise ValueError("OWNED_FIXTURE_TEARDOWN_CONTAINMENT")
    paths = [root, *root.rglob("*")]
    if len(paths) > 4096:
        raise ValueError("OWNED_FIXTURE_TEARDOWN_ENTRY_LIMIT")
    for path in paths:
        info = path.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise ValueError("OWNED_FIXTURE_TEARDOWN_REPARSE_POINT")
        if not path.resolve(strict=True).is_relative_to(root):
            raise ValueError("OWNED_FIXTURE_TEARDOWN_CONTAINMENT")

    def retry_readonly_file(function, path, error):
        path = Path(path)
        if not path.resolve(strict=True).is_relative_to(root):
            raise ValueError("OWNED_FIXTURE_TEARDOWN_CONTAINMENT") from error
        info = path.lstat()
        if (not isinstance(error, PermissionError) or not stat.S_ISREG(info.st_mode)
                or info.st_mode & stat.S_IWRITE
                or getattr(info, "st_file_attributes", 0) & 0x400):
            raise error
        os.chmod(path, info.st_mode | stat.S_IWRITE)
        function(path)

    shutil.rmtree(root, onexc=retry_readonly_file)
