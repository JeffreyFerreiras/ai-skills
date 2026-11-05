"""Compare JSON leaf types and float bits without recursive traversal."""

import math
import struct


def exact_equal(left, right):
    pending = [(left, right)]
    while pending:
        first, second = pending.pop()
        if type(first) is not type(second):
            return False
        if isinstance(first, float):
            if math.isnan(first) and math.isnan(second):
                continue
            if struct.pack(">d", first) != struct.pack(">d", second):
                return False
        elif isinstance(first, dict):
            if first.keys() != second.keys():
                return False
            pending.extend((value, second[key]) for key, value in first.items())
        elif isinstance(first, list):
            if len(first) != len(second):
                return False
            pending.extend(zip(first, second))
        elif first != second:
            return False
    return True
