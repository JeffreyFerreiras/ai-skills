"""Regress actual deep-value comparison failure without relaxing leaf checks."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0,str(Path(__file__).resolve().parent.parent/"parity"))
from comparison import exact_equal


class ComparisonTests(unittest.TestCase):
    def test_deep_values_match_and_leaf_types_remain_distinct(self):
        first,second,different=1,1,1.0
        for _ in range(2997):
            first=[first];second=[second];different=[different]
        self.assertTrue(exact_equal(first,second))
        self.assertFalse(exact_equal(first,different))

    def test_float_bits_and_boolean_types_remain_distinct(self):
        self.assertFalse(exact_equal(-0.0,0.0))
        self.assertFalse(exact_equal(False,0))
        self.assertFalse(exact_equal(1,1.0))
        self.assertTrue(exact_equal(float("nan"),float("nan")))
        self.assertFalse(exact_equal(float("inf"),float("-inf")))

    def test_fields_lengths_and_order_remain_checked(self):
        self.assertFalse(exact_equal({"a":1},{"b":1}))
        self.assertFalse(exact_equal([1],[1,2]))
        self.assertFalse(exact_equal([1,2],[2,1]))


if __name__=="__main__":unittest.main()
