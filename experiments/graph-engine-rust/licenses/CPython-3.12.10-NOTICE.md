The powersort implementation in `src/domain/python_sort.rs` derives from
CPython 3.12.10 `Objects/listobject.c`, commit
`0cc81280367df838c4b199f8f0378837165071c2`.

Upstream: https://github.com/python/cpython/blob/0cc81280367df838c4b199f8f0378837165071c2/Objects/listobject.c

Changes: translated run detection, binary insertion, galloping, merging and
powersort scheduling to safe Rust over index permutations. Object ownership,
comparison callbacks and error handling use the native Rust value model.
The implementation does not call or embed the Python runtime.

The relative-path error wording in `src/application/graph_initialization.rs`
follows CPython 3.12.10 `Lib/pathlib.py`. The Rust implementation uses the
native path port and preserves platform path comparison and code points.

The seconds-to-timedelta behavior in `src/domain/python_datetime.rs` derives
from CPython 3.12.10 `Modules/_datetimemodule.c` at the same pinned commit.
Changes: translated integer/fraction accumulation, half-even microsecond
rounding, floor normalization and range-check order to safe Rust. Calendar
addition uses the Rust `time` library; no Python runtime is embedded or called.
The ISO lease parser in `src/domain/python_isoformat.rs` also derives from
that file's separator, surrogate sanitizer and date/time/offset parsing.
The Rust `time` library supplies Gregorian and ISO-week calendar validation.

Upstream: https://github.com/python/cpython/blob/0cc81280367df838c4b199f8f0378837165071c2/Modules/_datetimemodule.c

The upstream license and copyright notices are retained in
`CPython-3.12.10-LICENSE`.
