"""Source-set locks and deterministic checkpoints, generalized from production."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import zipfile

EXCLUDED = {'.git', '__pycache__', 'node_modules', '.venv', 'venv', 'models', '.hf-cache'}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def capture(root):
    root = Path(root).resolve()
    files = {}
    for path in sorted(root.rglob('*')):
        relative = path.relative_to(root)
        if relative.as_posix() == 'CHECKPOINT-MANIFEST.json':
            continue
        if any(p in EXCLUDED for p in relative.parts) or path.name == '.render.lock':
            continue
        if path.is_symlink():
            raise ValueError(f'Symlink input is unsupported: {relative}')
        if path.is_file():
            files[relative.as_posix()] = digest(path.read_bytes())
    if not files:
        raise ValueError('Empty source project')
    return {'version': 1, 'sources': files,
            'source_set_sha256': digest(json.dumps(files, sort_keys=True, separators=(',', ':')).encode())}


def validate(root, lock):
    current = capture(root)
    if current != lock:
        old, new = lock['sources'], current['sources']
        changed = sorted(n for n in old.keys() | new.keys() if old.get(n) != new.get(n))
        raise ValueError('Source set changed: ' + ', '.join(changed))
    return current


def outside(root, target):
    target = Path(target).resolve()
    if target.is_relative_to(Path(root).resolve()):
        raise ValueError('Keep output, lock, checkpoint and render work outside the source project')
    return target


@contextmanager
def ownership(path):
    """Portable exclusive ownership. Never remove a lock owned by another run."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    try:
        os.write(fd, str(os.getpid()).encode())
        yield
    finally:
        os.close(fd)
        path.unlink()


def checkpoint(root, output, lock):
    root = Path(root).resolve()
    output = outside(root, output)
    output.parent.mkdir(parents=True, exist_ok=True)
    validate(root, lock)
    partial = output.with_name(output.name + '.partial')
    with ownership(output.with_name(output.name + '.lock')):
        if output.exists():
            raise FileExistsError(output)
        try:
            with zipfile.ZipFile(partial, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
                entries = {n: (root / n).read_bytes() for n in lock['sources']}
                entries['CHECKPOINT-MANIFEST.json'] = (json.dumps(lock, indent=2) + '\n').encode()
                for name in sorted(entries):
                    info = zipfile.ZipInfo(name, (2020, 1, 1, 0, 0, 0))
                    info.compress_type = zipfile.ZIP_DEFLATED
                    info.external_attr = 0o100644 << 16
                    archive.writestr(info, entries[name])
            validate(root, lock)
            with zipfile.ZipFile(partial) as archive:
                if archive.testzip() is not None:
                    raise ValueError('Corrupt checkpoint')
                for name, expected in lock['sources'].items():
                    if digest(archive.read(name)) != expected:
                        raise ValueError(f'Checkpoint readback mismatch: {name}')
            os.replace(partial, output)
        finally:
            partial.unlink(missing_ok=True)
    return {'path': str(output), 'sha256': digest(output.read_bytes()), 'files': len(lock['sources'])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['lock', 'check', 'checkpoint'])
    parser.add_argument('root', type=Path)
    parser.add_argument('lock', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    outside(args.root, args.lock)
    if args.action == 'lock':
        if args.lock.exists():
            raise FileExistsError('Do not overwrite a reviewed lock; create a new revision')
        lock = capture(args.root)
        args.lock.parent.mkdir(parents=True, exist_ok=True)
        args.lock.write_text(json.dumps(lock, indent=2) + '\n', encoding='utf-8')
        result = lock
    else:
        lock = json.loads(args.lock.read_text(encoding='utf-8'))
        result = validate(args.root, lock)
        if args.action == 'checkpoint':
            if not args.output:
                parser.error('checkpoint requires --output')
            result = checkpoint(args.root, args.output, lock)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
